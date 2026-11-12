// Keyboard decoding.

#include "cable.hpp"

#include <cerrno>

#ifdef _WIN32
#include <windows.h>

namespace {

std::optional<Key> translateConsoleKey(const KEY_EVENT_RECORD& event) {
    if (!event.bKeyDown) {
        return std::nullopt;
    }
    Key key;
    key.ctrl = (event.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED)) != 0;
    key.alt = (event.dwControlKeyState & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED)) != 0;
    key.shift = (event.dwControlKeyState & SHIFT_PRESSED) != 0;

    switch (event.wVirtualKeyCode) {
    case VK_UP:
        key.code = KeyCode::Up;
        return key;
    case VK_DOWN:
        key.code = KeyCode::Down;
        return key;
    case VK_LEFT:
        key.code = KeyCode::Left;
        return key;
    case VK_RIGHT:
        key.code = KeyCode::Right;
        return key;
    case VK_HOME:
        key.code = KeyCode::Home;
        return key;
    case VK_END:
        key.code = KeyCode::End;
        return key;
    case VK_PRIOR:
        key.code = KeyCode::PageUp;
        return key;
    case VK_NEXT:
        key.code = KeyCode::PageDown;
        return key;
    case VK_INSERT:
        key.code = KeyCode::Insert;
        return key;
    case VK_DELETE:
        key.code = KeyCode::Delete;
        return key;
    case VK_TAB:
        key.code = key.shift ? KeyCode::BackTab : KeyCode::Tab;
        return key;
    case VK_RETURN:
        key.code = KeyCode::Enter;
        return key;
    case VK_BACK:
        key.code = KeyCode::Backspace;
        return key;
    case VK_ESCAPE:
        key.code = KeyCode::Esc;
        return key;
    case VK_SCROLL:
        key.code = KeyCode::ScrollLock;
        return key;
    case VK_F1:
    case VK_F2:
    case VK_F3:
    case VK_F4:
    case VK_F5:
    case VK_F6:
    case VK_F7:
    case VK_F8:
    case VK_F9:
    case VK_F10:
    case VK_F11:
    case VK_F12:
        key.code = static_cast<KeyCode>(
            static_cast<int>(KeyCode::F1) + (event.wVirtualKeyCode - VK_F1));
        return key;
    default:
        break;
    }

    wchar_t ch = event.uChar.UnicodeChar;
    if (ch == 0) {
        return std::nullopt;
    }
    if (ch == L'\t') {
        key.code = key.shift ? KeyCode::BackTab : KeyCode::Tab;
        return key;
    }
    if (ch == L'\r' || ch == L'\n') {
        key.code = KeyCode::Enter;
        return key;
    }
    if (ch == L'\b') {
        key.code = KeyCode::Backspace;
        return key;
    }
    if (ch == 27) {
        key.code = KeyCode::Esc;
        return key;
    }
    if (ch < 32) {
        // Ctrl+letter arrives as a control character.
        key.code = KeyCode::Rune;
        key.rune = static_cast<char32_t>(U'a' + ch - 1);
        key.ctrl = true;
        return key;
    }
    key.code = KeyCode::Rune;
    key.rune = static_cast<char32_t>(ch);
    return key;
}

} // namespace
#else
#include <poll.h>
#include <unistd.h>
#endif

bool isScrollLockToggle(const Key& key) {
    if (key.code == KeyCode::ScrollLock || (key.code == KeyCode::F7)) {
        return true;
    }
    if (key.code == KeyCode::Rune) {
        if (key.rune == U'\f') {
            return true;
        }
        if (key.ctrl) {
            switch (key.rune) {
            case U'l':
            case U'L':
            case U'b':
            case U'B':
                return true;
            default:
                break;
            }
        }
    }
    return false;
}

static Key runeKey(char32_t rune, bool ctrl = false) {
    Key key;
    key.code = KeyCode::Rune;
    key.rune = rune;
    key.ctrl = ctrl;
    return key;
}

static Key codeKey(KeyCode code) {
    Key key;
    key.code = code;
    return key;
}

bool KeyReader::readByte(int timeoutMs, unsigned char& out) {
#ifdef _WIN32
    HANDLE handle = GetStdHandle(STD_INPUT_HANDLE);
    if (handle == INVALID_HANDLE_VALUE) {
        return false;
    }
    DWORD wait = WaitForSingleObject(handle, timeoutMs < 0 ? INFINITE : static_cast<DWORD>(timeoutMs));
    if (wait != WAIT_OBJECT_0) {
        return false;
    }
    DWORD got = 0;
    char c = 0;
    if (!ReadFile(handle, &c, 1, &got, nullptr) || got != 1) {
        return false;
    }
    out = static_cast<unsigned char>(c);
    return true;
#else
    struct pollfd pfd;
    pfd.fd = 0;
    pfd.events = POLLIN;
    pfd.revents = 0;
    for (;;) {
        int r = ::poll(&pfd, 1, timeoutMs);
        if (r < 0) {
            if (errno == EINTR) {
                continue;
            }
            return false;
        }
        if (r == 0) {
            return false;
        }
        break;
    }
    unsigned char c = 0;
    ssize_t n = ::read(0, &c, 1);
    if (n != 1) {
        return false;
    }
    out = c;
    return true;
#endif
}

Key KeyReader::decodeByte(unsigned char b) {
    switch (b) {
    case 9:
        return codeKey(KeyCode::Tab);
    case 13:
        return codeKey(KeyCode::Enter);
    case 8:
    case 127:
        return codeKey(KeyCode::Backspace);
    case 12:
        return runeKey(U'\f');
    default:
        break;
    }
    if (b > 0 && b < 27) {
        return runeKey(static_cast<char32_t>(U'a' + b - 1), true);
    }
    if (b >= 0x20 && b < 0x80) {
        return runeKey(static_cast<char32_t>(b));
    }
    if (b >= 0x80) {
        std::string buf;
        buf.push_back(static_cast<char>(b));
        int need = 0;
        if ((b & 0xE0) == 0xC0) {
            need = 1;
        } else if ((b & 0xF0) == 0xE0) {
            need = 2;
        } else if ((b & 0xF8) == 0xF0) {
            need = 3;
        }
        for (int i = 0; i < need; ++i) {
            unsigned char nb = 0;
            if (!readByte(20, nb)) {
                break;
            }
            buf.push_back(static_cast<char>(nb));
        }
        auto runes = utf8Decode(buf);
        if (runes.size() == 1 && runes[0] != 0xFFFD) {
            return runeKey(runes[0]);
        }
    }
    return Key{};
}

std::optional<Key> KeyReader::readCSI() {
    std::string params;
    for (;;) {
        unsigned char b = 0;
        if (!readByte(30, b)) {
            return codeKey(KeyCode::Esc);
        }
        if (b >= 0x40 && b <= 0x7E) {
            int n = 0;
            if (b == '~') {
                for (char ch : params) {
                    if (ch < '0' || ch > '9') {
                        break;
                    }
                    n = n * 10 + (ch - '0');
                }
            }
            switch (b) {
            case 'A':
                return codeKey(KeyCode::Up);
            case 'B':
                return codeKey(KeyCode::Down);
            case 'C':
                return codeKey(KeyCode::Right);
            case 'D':
                return codeKey(KeyCode::Left);
            case 'H':
                return codeKey(KeyCode::Home);
            case 'F':
                return codeKey(KeyCode::End);
            case 'Z':
                return codeKey(KeyCode::BackTab);
            case 'P':
                return codeKey(KeyCode::F1);
            case 'Q':
                return codeKey(KeyCode::F2);
            case 'R':
                return codeKey(KeyCode::F3);
            case 'S':
                return codeKey(KeyCode::F4);
            case '~':
                switch (n) {
                case 1:
                case 7:
                    return codeKey(KeyCode::Home);
                case 2:
                    return codeKey(KeyCode::Insert);
                case 3:
                    return codeKey(KeyCode::Delete);
                case 4:
                case 8:
                    return codeKey(KeyCode::End);
                case 5:
                    return codeKey(KeyCode::PageUp);
                case 6:
                    return codeKey(KeyCode::PageDown);
                case 11:
                    return codeKey(KeyCode::F1);
                case 12:
                    return codeKey(KeyCode::F2);
                case 13:
                    return codeKey(KeyCode::F3);
                case 14:
                    return codeKey(KeyCode::F4);
                case 15:
                    return codeKey(KeyCode::F5);
                case 17:
                    return codeKey(KeyCode::F6);
                case 18:
                    return codeKey(KeyCode::F7);
                case 19:
                    return codeKey(KeyCode::F8);
                case 20:
                    return codeKey(KeyCode::F9);
                case 21:
                    return codeKey(KeyCode::F10);
                case 23:
                    return codeKey(KeyCode::F11);
                case 24:
                    return codeKey(KeyCode::F12);
                default:
                    break;
                }
                break;
            default:
                break;
            }
            return codeKey(KeyCode::Esc);
        }
        if (params.size() < 16) {
            params.push_back(static_cast<char>(b));
        }
    }
}

std::optional<Key> KeyReader::readSS3() {
    unsigned char b = 0;
    if (!readByte(30, b)) {
        return codeKey(KeyCode::Esc);
    }
    switch (b) {
    case 'A':
        return codeKey(KeyCode::Up);
    case 'B':
        return codeKey(KeyCode::Down);
    case 'C':
        return codeKey(KeyCode::Right);
    case 'D':
        return codeKey(KeyCode::Left);
    case 'H':
        return codeKey(KeyCode::Home);
    case 'F':
        return codeKey(KeyCode::End);
    case 'P':
        return codeKey(KeyCode::F1);
    case 'Q':
        return codeKey(KeyCode::F2);
    case 'R':
        return codeKey(KeyCode::F3);
    case 'S':
        return codeKey(KeyCode::F4);
    default:
        return codeKey(KeyCode::Esc);
    }
}

std::optional<Key> KeyReader::parseFirst(unsigned char b) {
    if (b == 0x1B) {
        unsigned char c = 0;
        if (!readByte(30, c)) {
            return codeKey(KeyCode::Esc);
        }
        if (c == '[') {
            return readCSI();
        }
        if (c == 'O') {
            return readSS3();
        }
        return decodeByte(c);
    }
    return decodeByte(b);
}

std::optional<Key> KeyReader::readKey() {
    unsigned char b = 0;
    if (!readByte(0, b)) {
        return std::nullopt;
    }
    return parseFirst(b);
}

std::optional<Key> KeyReader::poll(int timeoutMs) {
#ifdef _WIN32
    if (!pending_.empty()) {
        Key key = pending_.front();
        pending_.pop_front();
        return key;
    }
    HANDLE handle = GetStdHandle(STD_INPUT_HANDLE);
    if (handle == INVALID_HANDLE_VALUE) {
        return std::nullopt;
    }
    DWORD start = GetTickCount();
    for (;;) {
        DWORD waitMs = INFINITE;
        if (timeoutMs > 0) {
            DWORD elapsed = GetTickCount() - start;
            if (elapsed >= static_cast<DWORD>(timeoutMs)) {
                return std::nullopt;
            }
            waitMs = static_cast<DWORD>(timeoutMs) - elapsed;
        } else if (timeoutMs == 0) {
            waitMs = 0;
        }
        DWORD wait = WaitForSingleObject(handle, waitMs);
        if (wait != WAIT_OBJECT_0) {
            return std::nullopt;
        }
        INPUT_RECORD record;
        DWORD got = 0;
        if (!ReadConsoleInputW(handle, &record, 1, &got) || got == 0) {
            return std::nullopt;
        }
        if (record.EventType != KEY_EVENT) {
            continue;
        }
        std::optional<Key> key = translateConsoleKey(record.Event.KeyEvent);
        if (!key) {
            continue;
        }
        WORD repeats = record.Event.KeyEvent.wRepeatCount;
        for (WORD i = 1; i < repeats; ++i) {
            pending_.push_back(*key);
        }
        return key;
    }
#else
    unsigned char b = 0;
    if (!readByte(timeoutMs, b)) {
        return std::nullopt;
    }
    return parseFirst(b);
#endif
}
