// Keyboard decoding.

#include "cable.hpp"

#include <cerrno>

#ifdef _WIN32
#include <windows.h>
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
    unsigned char b = 0;
    if (!readByte(timeoutMs, b)) {
        return std::nullopt;
    }
    return parseFirst(b);
}
