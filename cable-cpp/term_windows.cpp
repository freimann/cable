// Windows console handling.
//
// Windows 10 understands ANSI escape sequences (virtual terminal processing).
// Windows 7 and 8 do not: SetConsoleMode rejects the VT flags and the console
// prints escapes verbatim.  When that happens we fall back to translating the
// escape stream ourselves with the classic console API (cursor positioning,
// SetConsoleTextAttribute and WriteConsoleW).  Console input is always read as
// INPUT_RECORDs so it works on every Windows version.

#ifdef _WIN32

#include "cable.hpp"

#include <cstdlib>
#include <windows.h>

namespace {
bool g_active = false;
bool g_legacyOut = false;
DWORD g_inMode = 0;
DWORD g_outMode = 0;
UINT g_oldCP = 0;

HANDLE g_out = INVALID_HANDLE_VALUE;
HANDLE g_origOut = INVALID_HANDLE_VALUE;
bool g_altBuffer = false;
WORD g_defaultAttr = FOREGROUND_RED | FOREGROUND_GREEN | FOREGROUND_BLUE;
WORD g_curAttr = FOREGROUND_RED | FOREGROUND_GREEN | FOREGROUND_BLUE;
bool g_bold = false;
COORD g_cursor = {0, 0};
CONSOLE_CURSOR_INFO g_oldCursorInfo = {25, TRUE};

constexpr DWORD kEnableProcessedInput = 0x0001;
constexpr DWORD kEnableLineInput = 0x0002;
constexpr DWORD kEnableEchoInput = 0x0004;
constexpr DWORD kEnableWindowInput = 0x0008;
constexpr DWORD kEnableQuickEdit = 0x0040;
constexpr DWORD kEnableExtendedFlags = 0x0080;
constexpr DWORD kEnableProcessedOutput = 0x0001;
constexpr DWORD kEnableWrapAtEol = 0x0002;
constexpr DWORD kEnableVirtualTerminalProcessing = 0x0004;

COORD bufferPosition(COORD window) {
    CONSOLE_SCREEN_BUFFER_INFO info {};
    if (GetConsoleScreenBufferInfo(g_out, &info)) {
        return {static_cast<SHORT>(info.srWindow.Left + window.X),
                static_cast<SHORT>(info.srWindow.Top + window.Y)};
    }
    return window;
}

void moveCursor(COORD window) { SetConsoleCursorPosition(g_out, bufferPosition(window)); }

void writeOut(const std::string& text);

// ---------------------------------------------------------------------------
// ANSI-to-console translation (Windows 7/8 fallback)

struct Rgb {
    int r;
    int g;
    int b;
};

constexpr Rgb kAnsiPalette[16] = {
    {0, 0, 0},       {128, 0, 0},     {0, 128, 0},     {128, 128, 0},
    {0, 0, 128},     {128, 0, 128},   {0, 128, 128},   {192, 192, 192},
    {128, 128, 128}, {255, 0, 0},     {0, 255, 0},     {255, 255, 0},
    {0, 0, 255},     {255, 0, 255},   {0, 255, 255},   {255, 255, 255},
};

int nearestAnsi16(int r, int g, int b) {
    int best = 0;
    long bestDistance = -1;
    for (int i = 0; i < 16; ++i) {
        long dr = r - kAnsiPalette[i].r;
        long dg = g - kAnsiPalette[i].g;
        long db = b - kAnsiPalette[i].b;
        long distance = dr * dr + dg * dg + db * db;
        if (bestDistance < 0 || distance < bestDistance) {
            bestDistance = distance;
            best = i;
        }
    }
    return best;
}

int ansi256ToAnsi16(int n) {
    if (n < 0) {
        return 0;
    }
    if (n < 16) {
        return n;
    }
    if (n < 232) {
        static const int levels[6] = {0, 95, 135, 175, 215, 255};
        int v = n - 16;
        int r = levels[v / 36];
        int g = levels[(v / 6) % 6];
        int b = levels[v % 6];
        return nearestAnsi16(r, g, b);
    }
    int v = 8 + (n - 232) * 10;
    return nearestAnsi16(v, v, v);
}

WORD ansiFgBits(int index) {
    WORD attr = 0;
    if (index & 1) {
        attr |= FOREGROUND_RED;
    }
    if (index & 2) {
        attr |= FOREGROUND_GREEN;
    }
    if (index & 4) {
        attr |= FOREGROUND_BLUE;
    }
    if (index & 8) {
        attr |= FOREGROUND_INTENSITY;
    }
    return attr;
}

WORD ansiBgBits(int index) {
    WORD attr = 0;
    if (index & 1) {
        attr |= BACKGROUND_RED;
    }
    if (index & 2) {
        attr |= BACKGROUND_GREEN;
    }
    if (index & 4) {
        attr |= BACKGROUND_BLUE;
    }
    if (index & 8) {
        attr |= BACKGROUND_INTENSITY;
    }
    return attr;
}

void setFg(int index) {
    WORD bits = ansiFgBits(index);
    if (g_bold) {
        bits |= FOREGROUND_INTENSITY;
    }
    g_curAttr = static_cast<WORD>((g_curAttr & ~0x000F) | bits);
}
void setBg(int index) { g_curAttr = static_cast<WORD>((g_curAttr & ~0x00F0) | ansiBgBits(index)); }

std::vector<int> parseParams(const std::string& params) {
    std::vector<int> values;
    if (params.empty()) {
        return values;
    }
    int value = 0;
    bool digits = false;
    for (char ch : params) {
        if (ch >= '0' && ch <= '9') {
            value = value * 10 + (ch - '0');
            digits = true;
        } else if (ch == ';' || ch == ':') {
            values.push_back(digits ? value : 0);
            value = 0;
            digits = false;
        }
    }
    values.push_back(digits ? value : 0);
    return values;
}

void applySgr(const std::string& params) {
    std::vector<int> values = parseParams(params);
    if (values.empty()) {
        g_curAttr = g_defaultAttr;
        g_bold = false;
        SetConsoleTextAttribute(g_out, g_curAttr);
        return;
    }
    for (std::size_t i = 0; i < values.size(); ++i) {
        int n = values[i];
        if (n == 0) {
            g_curAttr = g_defaultAttr;
            g_bold = false;
        } else if (n == 1) {
            g_bold = true;
            g_curAttr |= FOREGROUND_INTENSITY;
        } else if (n == 22) {
            g_bold = false;
            g_curAttr &= static_cast<WORD>(~FOREGROUND_INTENSITY);
        } else if (n >= 30 && n <= 37) {
            setFg(n - 30);
        } else if (n >= 90 && n <= 97) {
            setFg(n - 90 + 8);
        } else if (n >= 40 && n <= 47) {
            setBg(n - 40);
        } else if (n >= 100 && n <= 107) {
            setBg(n - 100 + 8);
        } else if (n == 39) {
            g_curAttr = static_cast<WORD>((g_curAttr & ~0x000F) | (g_defaultAttr & 0x000F));
        } else if (n == 49) {
            g_curAttr = static_cast<WORD>((g_curAttr & ~0x00F0) | (g_defaultAttr & 0x00F0));
        } else if ((n == 38 || n == 48) && i + 1 < values.size()) {
            bool foreground = n == 38;
            if (values[i + 1] == 5 && i + 2 < values.size()) {
                int index = ansi256ToAnsi16(values[i + 2]);
                if (foreground) {
                    setFg(index);
                } else {
                    setBg(index);
                }
                i += 2;
            } else if (values[i + 1] == 2 && i + 4 < values.size()) {
                int index = nearestAnsi16(values[i + 2], values[i + 3], values[i + 4]);
                if (foreground) {
                    setFg(index);
                } else {
                    setBg(index);
                }
                i += 4;
            }
        }
    }
    SetConsoleTextAttribute(g_out, g_curAttr);
}

void writeText(const std::string& text) {
    std::wstring run;
    COORD runPos = g_cursor;
    auto flushRun = [&]() {
        if (run.empty()) {
            return;
        }
        moveCursor(runPos);
        DWORD written = 0;
        WriteConsoleW(g_out, run.data(), static_cast<DWORD>(run.size()), &written, nullptr);
        run.clear();
    };
    for (char32_t cp : utf8Decode(text)) {
        if (cp == U'\r' || cp == U'\n') {
            flushRun();
            if (cp == U'\n' && g_cursor.Y < 32767) {
                ++g_cursor.Y;
            }
            g_cursor.X = 0;
            continue;
        }
        if (run.empty()) {
            runPos = g_cursor;
        }
        if (cp < 0x10000) {
            run.push_back(static_cast<wchar_t>(cp));
        } else {
            cp -= 0x10000;
            run.push_back(static_cast<wchar_t>(0xD800 + (cp >> 10)));
            run.push_back(static_cast<wchar_t>(0xDC00 + (cp & 0x3FF)));
        }
        ++g_cursor.X;
    }
    flushRun();
}

void showCursor(bool visible) {
    CONSOLE_CURSOR_INFO info;
    if (GetConsoleCursorInfo(g_out, &info)) {
        info.bVisible = visible ? TRUE : FALSE;
        SetConsoleCursorInfo(g_out, &info);
    }
}

void setAutoWrap(bool on) {
    DWORD mode = 0;
    if (!GetConsoleMode(g_out, &mode)) {
        return;
    }
    if (on) {
        mode |= kEnableWrapAtEol;
    } else {
        mode &= ~kEnableWrapAtEol;
    }
    SetConsoleMode(g_out, mode);
}

void clearScreen() {
    CONSOLE_SCREEN_BUFFER_INFO info {};
    if (!GetConsoleScreenBufferInfo(g_out, &info)) {
        return;
    }
    DWORD cells = static_cast<DWORD>(info.dwSize.X) * static_cast<DWORD>(info.dwSize.Y);
    COORD home = {0, 0};
    DWORD written = 0;
    FillConsoleOutputCharacterW(g_out, L' ', cells, home, &written);
    FillConsoleOutputAttribute(g_out, g_defaultAttr, cells, home, &written);
    moveCursor(home);
    g_cursor = home;
}

void handleCsi(const std::string& params, char final) {
    if (!params.empty() && params[0] == '?') {
        int n = std::atoi(params.c_str() + 1);
        if (n == 25) {
            showCursor(final == 'h');
        } else if (n == 7) {
            setAutoWrap(final == 'h');
        }
        // 1049 is handled by activating a private screen buffer in start().
        return;
    }
    switch (final) {
    case 'H':
    case 'f': {
        int row = 1;
        int col = 1;
        std::vector<int> values = parseParams(params);
        if (values.size() >= 1 && values[0] > 0) {
            row = values[0];
        }
        if (values.size() >= 2 && values[1] > 0) {
            col = values[1];
        }
        g_cursor.X = static_cast<SHORT>(col - 1);
        g_cursor.Y = static_cast<SHORT>(row - 1);
        moveCursor(g_cursor);
        break;
    }
    case 'J': {
        int n = params.empty() ? 0 : std::atoi(params.c_str());
        if (n == 2 || n == 3) {
            clearScreen();
        }
        break;
    }
    case 'm':
        applySgr(params);
        break;
    default:
        break;
    }
}

void legacyWrite(const std::string& text) {
    std::size_t i = 0;
    while (i < text.size()) {
        if (static_cast<unsigned char>(text[i]) != 0x1B) {
            std::size_t start = i;
            while (i < text.size() && static_cast<unsigned char>(text[i]) != 0x1B) {
                ++i;
            }
            writeText(text.substr(start, i - start));
            continue;
        }
        if (i + 1 < text.size() && text[i + 1] == '[') {
            std::size_t j = i + 2;
            while (j < text.size() && !(text[j] >= 0x40 && text[j] <= 0x7E)) {
                ++j;
            }
            if (j >= text.size()) {
                break;
            }
            handleCsi(text.substr(i + 2, j - (i + 2)), text[j]);
            i = j + 1;
            continue;
        }
        // Unknown escape: skip it.
        i += (i + 1 < text.size()) ? 2 : 1;
    }
}

} // namespace

void writeTerminal(const std::string& text) {
    if (g_legacyOut) {
        legacyWrite(text);
        return;
    }
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD written = 0;
    WriteFile(out, text.data(), static_cast<DWORD>(text.size()), &written, nullptr);
}

namespace {

void writeOut(const std::string& text) { writeTerminal(text); }

} // namespace

std::unique_ptr<Terminal> Terminal::start(std::string& error) {
    HANDLE in = GetStdHandle(STD_INPUT_HANDLE);
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    if (in == INVALID_HANDLE_VALUE || out == INVALID_HANDLE_VALUE) {
        error = "Error opening console";
        return nullptr;
    }
    if (!GetConsoleMode(in, &g_inMode) || !GetConsoleMode(out, &g_outMode)) {
        error = "Error reading console mode";
        return nullptr;
    }
    g_out = out;

    CONSOLE_SCREEN_BUFFER_INFO screenInfo {};
    if (GetConsoleScreenBufferInfo(out, &screenInfo)) {
        g_defaultAttr = screenInfo.wAttributes;
    }
    g_curAttr = g_defaultAttr;
    (void)GetConsoleCursorInfo(out, &g_oldCursorInfo);

    // Read keys as INPUT_RECORDs, which works on every supported Windows
    // version; virtual-terminal input is a Windows 10 feature.
    DWORD newIn = g_inMode &
                  ~(kEnableProcessedInput | kEnableLineInput | kEnableEchoInput |
                    kEnableWindowInput);
    newIn |= kEnableExtendedFlags;
    newIn &= ~kEnableQuickEdit;
    if (!SetConsoleMode(in, newIn)) {
        newIn = g_inMode & ~(kEnableLineInput | kEnableEchoInput);
        if (!SetConsoleMode(in, newIn)) {
            if (!SetConsoleMode(in, g_inMode)) {
                error = "Error setting console input mode";
                return nullptr;
            }
        }
    }

    // Prefer virtual-terminal output (Windows 10).  Windows 7/8 reject the
    // flag; fall back to our own escape-sequence translator.
    DWORD newOut = g_outMode | kEnableProcessedOutput | kEnableVirtualTerminalProcessing;
    DWORD confirmedOut = 0;
    g_legacyOut = false;
    if (!SetConsoleMode(out, newOut) || !GetConsoleMode(out, &confirmedOut) ||
        (confirmedOut & kEnableVirtualTerminalProcessing) == 0) {
        DWORD legacyOut = (g_outMode | kEnableProcessedOutput) & ~kEnableWrapAtEol;
        if (!SetConsoleMode(out, legacyOut)) {
            SetConsoleMode(in, g_inMode);
            error = "Error setting console output mode";
            return nullptr;
        }
        g_legacyOut = true;
    }

    // On legacy consoles use a private screen buffer sized to the current
    // window.  It emulates the alternate screen: the user's scrollback stays
    // untouched and is restored when the program exits.
    if (g_legacyOut) {
        CONSOLE_SCREEN_BUFFER_INFO info {};
        if (GetConsoleScreenBufferInfo(out, &info)) {
            SHORT width = static_cast<SHORT>(info.srWindow.Right - info.srWindow.Left + 1);
            SHORT height = static_cast<SHORT>(info.srWindow.Bottom - info.srWindow.Top + 1);
            if (width < 1 || height < 1) {
                width = 80;
                height = 25;
            }
            HANDLE alt = CreateConsoleScreenBuffer(
                GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                CONSOLE_TEXTMODE_BUFFER, nullptr);
            if (alt != INVALID_HANDLE_VALUE) {
                SMALL_RECT tiny = {0, 0, 1, 1};
                COORD size = {width, height};
                SMALL_RECT full = {0, 0, static_cast<SHORT>(width - 1),
                                   static_cast<SHORT>(height - 1)};
                bool ready = SetConsoleWindowInfo(alt, TRUE, &tiny) != FALSE &&
                             SetConsoleScreenBufferSize(alt, size) != FALSE &&
                             SetConsoleWindowInfo(alt, TRUE, &full) != FALSE &&
                             SetConsoleActiveScreenBuffer(alt) != FALSE;
                if (ready) {
                    g_origOut = out;
                    g_out = alt;
                    g_altBuffer = true;
                } else {
                    CloseHandle(alt);
                }
            }
        }
    }

    g_oldCP = GetConsoleOutputCP();
    SetConsoleOutputCP(65001);
    if (g_altBuffer) {
        SetConsoleTextAttribute(g_out, g_curAttr);
    }

    auto terminal = std::unique_ptr<Terminal>(new Terminal());
    terminal->active_ = true;
    g_active = true;
    writeOut("\x1b[?1049h\x1b[?7l\x1b[?25l\x1b[2J");
    return terminal;
}

void Terminal::close() {
    if (!active_) {
        return;
    }
    active_ = false;
    writeOut("\x1b[?7h\x1b[?25h\x1b[?1049l");
    if (g_active) {
        if (g_altBuffer) {
            SetConsoleActiveScreenBuffer(g_origOut);
            CloseHandle(g_out);
            g_out = g_origOut;
            g_altBuffer = false;
        }
        if (g_legacyOut) {
            SetConsoleTextAttribute(g_out, g_defaultAttr);
            SetConsoleCursorInfo(g_out, &g_oldCursorInfo);
        }
        if (g_oldCP != 0) {
            SetConsoleOutputCP(g_oldCP);
        }
        SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), g_inMode);
        SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), g_outMode);
        g_active = false;
    }
}

Terminal::~Terminal() { close(); }

bool terminalSize(int& width, int& height) {
    HANDLE out = g_out != INVALID_HANDLE_VALUE ? g_out : GetStdHandle(STD_OUTPUT_HANDLE);
    CONSOLE_SCREEN_BUFFER_INFO info {};
    if (out == INVALID_HANDLE_VALUE || !GetConsoleScreenBufferInfo(out, &info)) {
        width = 80;
        height = 25;
        return true;
    }
    width = info.srWindow.Right - info.srWindow.Left + 1;
    height = info.srWindow.Bottom - info.srWindow.Top + 1;
    if (width < 1 || height < 1) {
        width = 80;
        height = 25;
    }
    return true;
}

#endif
