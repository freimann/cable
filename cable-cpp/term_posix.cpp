// POSIX terminal handling.

#ifndef _WIN32

#include "cable.hpp"

#include <cerrno>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

namespace {
bool g_active = false;
struct termios g_old {};
} // namespace

std::unique_ptr<Terminal> Terminal::start(std::string& error) {
    if (::tcgetattr(0, &g_old) != 0) {
        error = "Error reading terminal settings";
        return nullptr;
    }
    struct termios raw = g_old;
    raw.c_iflag &= ~(IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
    raw.c_oflag &= ~OPOST;
    raw.c_lflag &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    raw.c_cflag &= ~(CSIZE | PARENB);
    raw.c_cflag |= CS8;
    raw.c_cc[VMIN] = 1;
    raw.c_cc[VTIME] = 0;
    if (::tcsetattr(0, TCSANOW, &raw) != 0) {
        error = "Error setting terminal mode";
        return nullptr;
    }
    auto terminal = std::unique_ptr<Terminal>(new Terminal());
    terminal->active_ = true;
    g_active = true;
    std::string out = "\x1b[?1049h\x1b[?7l\x1b[?25l\x1b[2J";
    writeTerminal(out);
    return terminal;
}

void Terminal::close() {
    if (!active_) {
        return;
    }
    active_ = false;
    std::string out = "\x1b[?7h\x1b[?25h\x1b[?1049l";
    writeTerminal(out);
    if (g_active) {
        ::tcsetattr(0, TCSANOW, &g_old);
        g_active = false;
    }
}

Terminal::~Terminal() { close(); }

void writeTerminal(const std::string& text) {
    const char* data = text.data();
    std::size_t left = text.size();
    while (left > 0) {
        ssize_t n = ::write(1, data, left);
        if (n < 0) {
            if (errno == EINTR) {
                continue;
            }
            break;
        }
        data += n;
        left -= static_cast<std::size_t>(n);
    }
}

bool terminalSize(int& width, int& height) {
    struct winsize ws {};
    if (::ioctl(1, TIOCGWINSZ, &ws) != 0 || ws.ws_col == 0 || ws.ws_row == 0) {
        width = 80;
        height = 25;
        return true;
    }
    width = ws.ws_col;
    height = ws.ws_row;
    return true;
}

#endif
