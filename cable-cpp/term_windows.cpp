// Windows console handling.

#ifdef _WIN32

#include "cable.hpp"

#include <windows.h>

namespace {
bool g_active = false;
DWORD g_inMode = 0;
DWORD g_outMode = 0;
UINT g_oldCP = 0;

constexpr DWORD kEnableProcessedInput = 0x0001;
constexpr DWORD kEnableLineInput = 0x0002;
constexpr DWORD kEnableEchoInput = 0x0004;
constexpr DWORD kEnableWindowInput = 0x0008;
constexpr DWORD kEnableProcessedOutput = 0x0001;
constexpr DWORD kEnableVirtualTerminalInput = 0x0200;
constexpr DWORD kEnableVirtualTerminalProcessing = 0x0004;
constexpr DWORD kEnableExtendedFlags = 0x0080;

void writeOut(const std::string& text) {
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD written = 0;
    WriteFile(out, text.data(), static_cast<DWORD>(text.size()), &written, nullptr);
}
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
    DWORD newIn = g_inMode & ~(kEnableProcessedInput | kEnableLineInput | kEnableEchoInput |
                               kEnableWindowInput);
    newIn |= kEnableVirtualTerminalInput | kEnableExtendedFlags;
    DWORD newOut = g_outMode | kEnableProcessedOutput | kEnableVirtualTerminalProcessing;
    if (!SetConsoleMode(in, newIn)) {
        error = "Error setting console input mode";
        return nullptr;
    }
    if (!SetConsoleMode(out, newOut)) {
        SetConsoleMode(in, g_inMode);
        error = "Error setting console output mode";
        return nullptr;
    }
    g_oldCP = GetConsoleOutputCP();
    SetConsoleOutputCP(65001);

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
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
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
