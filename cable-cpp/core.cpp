// Core support: UTF-8 helpers, styles, paged file I/O, codecs, search,
// directory browsing and the passive disassembly fallback.

#include "cable.hpp"
#include "tables.hpp"

#include <cerrno>
#include <cctype>
#include <charconv>
#include <cstdlib>
#include <system_error>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#include <sys/stat.h>
#include <windows.h>
#else
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>
#endif

// ---------------------------------------------------------------------------
// UTF-8 helpers

std::vector<char32_t> utf8Decode(std::string_view text) {
    std::vector<char32_t> out;
    out.reserve(text.size());
    std::size_t i = 0;
    while (i < text.size()) {
        unsigned char b = static_cast<unsigned char>(text[i]);
        if (b < 0x80) {
            out.push_back(b);
            ++i;
            continue;
        }
        int need = 0;
        char32_t cp = 0;
        if ((b & 0xE0) == 0xC0) {
            need = 1;
            cp = b & 0x1F;
        } else if ((b & 0xF0) == 0xE0) {
            need = 2;
            cp = b & 0x0F;
        } else if ((b & 0xF8) == 0xF0) {
            need = 3;
            cp = b & 0x07;
        } else {
            out.push_back(0xFFFD);
            ++i;
            continue;
        }
        if (i + static_cast<std::size_t>(need) >= text.size()) {
            out.push_back(0xFFFD);
            break;
        }
        bool ok = true;
        for (int k = 1; k <= need; ++k) {
            unsigned char c = static_cast<unsigned char>(text[i + static_cast<std::size_t>(k)]);
            if ((c & 0xC0) != 0x80) {
                ok = false;
                break;
            }
            cp = (cp << 6) | (c & 0x3F);
        }
        if (!ok) {
            out.push_back(0xFFFD);
            ++i;
            continue;
        }
        out.push_back(cp);
        i += static_cast<std::size_t>(need) + 1;
    }
    return out;
}

std::string utf8Encode(char32_t c) {
    std::string out;
    if (c <= 0x7F) {
        out.push_back(static_cast<char>(c));
    } else if (c <= 0x7FF) {
        out.push_back(static_cast<char>(0xC0 | (c >> 6)));
        out.push_back(static_cast<char>(0x80 | (c & 0x3F)));
    } else if (c <= 0xFFFF) {
        out.push_back(static_cast<char>(0xE0 | (c >> 12)));
        out.push_back(static_cast<char>(0x80 | ((c >> 6) & 0x3F)));
        out.push_back(static_cast<char>(0x80 | (c & 0x3F)));
    } else {
        out.push_back(static_cast<char>(0xF0 | (c >> 18)));
        out.push_back(static_cast<char>(0x80 | ((c >> 12) & 0x3F)));
        out.push_back(static_cast<char>(0x80 | ((c >> 6) & 0x3F)));
        out.push_back(static_cast<char>(0x80 | (c & 0x3F)));
    }
    return out;
}

std::size_t runeLen(std::string_view text) {
    return utf8Decode(text).size();
}

std::string truncateRunes(std::string_view text, std::size_t width) {
    if (width == 0) {
        return {};
    }
    auto runes = utf8Decode(text);
    if (runes.size() <= width) {
        return std::string(text);
    }
    std::string out;
    for (std::size_t i = 0; i < width; ++i) {
        out += utf8Encode(runes[i]);
    }
    return out;
}

std::string padRight(std::string_view text, std::size_t width) {
    std::size_t n = runeLen(text);
    std::string out(text);
    if (n < width) {
        out.append(width - n, ' ');
    }
    return out;
}

std::string padLeft(std::string_view text, std::size_t width) {
    std::size_t n = runeLen(text);
    std::string out;
    if (n < width) {
        out.append(width - n, ' ');
    }
    out += text;
    return out;
}

std::string collapseSpaces(std::string_view text) {
    std::string out;
    bool space = false;
    for (unsigned char c : text) {
        if (c == ' ' || c == '\t' || c == '\r' || c == '\n') {
            space = true;
            continue;
        }
        if (space && !out.empty()) {
            out.push_back(' ');
        }
        space = false;
        out.push_back(static_cast<char>(c));
    }
    std::string trimmed;
    std::size_t begin = 0;
    while (begin < out.size() && out[begin] == ' ') {
        ++begin;
    }
    std::size_t end = out.size();
    while (end > begin && out[end - 1] == ' ') {
        --end;
    }
    return out.substr(begin, end - begin);
}

// ---------------------------------------------------------------------------
// Styles and screen

std::string fg256(int n) { return strprintf("38;5;%d", n); }
std::string bg256(int n) { return strprintf("48;5;%d", n); }
std::string fgRGB(int r, int g, int b) { return strprintf("38;2;%d;%d;%d", r, g, b); }
std::string bgRGB(int r, int g, int b) { return strprintf("48;2;%d;%d;%d", r, g, b); }

static std::string sgr(const Style& style) {
    std::string out = "\x1b[0";
    if (style.bold) {
        out += ";1";
    }
    if (!style.fg.empty()) {
        out += ";" + style.fg;
    }
    if (!style.bg.empty()) {
        out += ";" + style.bg;
    }
    out += "m";
    return out;
}

Screen::Screen(int width, int height, const Style& base) {
    w = std::max(width, 0);
    h = std::max(height, 0);
    cells.assign(static_cast<std::size_t>(w) * static_cast<std::size_t>(h), Cell{U' ', base});
}

void Screen::set(int x, int y, char32_t ch, const Style& style) {
    if (x < 0 || y < 0 || x >= w || y >= h) {
        return;
    }
    cells[static_cast<std::size_t>(y) * static_cast<std::size_t>(w) + static_cast<std::size_t>(x)] =
        Cell{ch, style};
}

int Screen::put(int x, int y, std::string_view text, const Style& style) {
    for (char32_t ch : utf8Decode(text)) {
        if (x >= w) {
            break;
        }
        set(x, y, ch, style);
        ++x;
    }
    return x;
}

int Screen::putLimit(int x, int y, std::string_view text, const Style& style, int limit) {
    int n = 0;
    for (char32_t ch : utf8Decode(text)) {
        if (n >= limit || x >= w) {
            break;
        }
        set(x, y, ch, style);
        ++x;
        ++n;
    }
    return x;
}

void Screen::restyle(const Rect& rect, const Style& style) {
    for (int y = rect.y; y < rect.y + rect.h; ++y) {
        for (int x = rect.x; x < rect.x + rect.w; ++x) {
            if (x < 0 || y < 0 || x >= w || y >= h) {
                continue;
            }
            cells[static_cast<std::size_t>(y) * static_cast<std::size_t>(w) +
                  static_cast<std::size_t>(x)]
                .style = style;
        }
    }
}

void Screen::fill(const Rect& rect, char32_t ch, const Style& style) {
    for (int y = rect.y; y < rect.y + rect.h; ++y) {
        for (int x = rect.x; x < rect.x + rect.w; ++x) {
            set(x, y, ch, style);
        }
    }
}

void Screen::flush(std::string& out) const {
    out += "\x1b[H";
    const Style* last = nullptr;
    Style previous;
    for (int y = 0; y < h; ++y) {
        out += strprintf("\x1b[%d;1H", y + 1);
        for (int x = 0; x < w; ++x) {
            const Cell& cell = cells[static_cast<std::size_t>(y) * static_cast<std::size_t>(w) +
                                     static_cast<std::size_t>(x)];
            if (last == nullptr || cell.style != previous) {
                out += sgr(cell.style);
                previous = cell.style;
                last = &previous;
            }
            out += utf8Encode(cell.ch);
        }
    }
    out += "\x1b[0m";
}

int satSub(int a, int b) { return a < b ? 0 : a - b; }
int maxInt(int a, int b) { return a > b ? a : b; }

std::vector<std::string> wrapText(std::string_view text, int width) {
    if (width <= 0) {
        return {std::string(text)};
    }
    std::vector<std::string> words;
    std::string word;
    for (char32_t ch : utf8Decode(text)) {
        if (ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n') {
            if (!word.empty()) {
                words.push_back(word);
                word.clear();
            }
        } else {
            word += utf8Encode(ch);
        }
    }
    if (!word.empty()) {
        words.push_back(word);
    }
    if (words.empty()) {
        return {""};
    }
    std::vector<std::string> lines;
    std::string line;
    for (const auto& w : words) {
        if (line.empty()) {
            line = w;
            continue;
        }
        if (static_cast<int>(runeLen(line) + 1 + runeLen(w)) <= width) {
            line += " " + w;
        } else {
            lines.push_back(line);
            line = w;
        }
    }
    lines.push_back(line);
    return lines;
}

// ---------------------------------------------------------------------------
// Paged file

namespace {

#ifdef _WIN32
int openReadOnly(const fs::path& path) {
    return _wopen(path.c_str(), _O_RDONLY | _O_BINARY);
}
void closeFd(int fd) { _close(fd); }
bool readAt(int fd, u64 offset, u8* data, std::size_t n, std::size_t& got) {
    got = 0;
    if (_lseeki64(fd, static_cast<__int64>(offset), SEEK_SET) < 0) {
        return false;
    }
    while (got < n) {
        int r = _read(fd, data + got, static_cast<unsigned int>(
                                          std::min<std::size_t>(n - got, 1u << 30)));
        if (r <= 0) {
            break;
        }
        got += static_cast<std::size_t>(r);
    }
    return true;
}
u64 fileLength(int fd) {
    struct _stat64 st;
    if (_fstat64(fd, &st) != 0) {
        return 0;
    }
    return static_cast<u64>(st.st_size);
}
bool modeIsReadOnly(const fs::path& path) {
    DWORD attrs = GetFileAttributesW(path.c_str());
    return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_READONLY) != 0;
}
int createTempFile(const fs::path& path) {
    return _wopen(path.c_str(), _O_WRONLY | _O_CREAT | _O_EXCL | _O_BINARY,
                  _S_IREAD | _S_IWRITE);
}
bool writeAll(int fd, const u8* data, std::size_t n) {
    std::size_t done = 0;
    while (done < n) {
        int r = _write(fd, data + done,
                       static_cast<unsigned int>(std::min<std::size_t>(n - done, 1u << 30)));
        if (r <= 0) {
            return false;
        }
        done += static_cast<std::size_t>(r);
    }
    return true;
}
bool syncFd(int fd) { return _commit(fd) == 0; }
#else
int openReadOnly(const fs::path& path) {
    return ::open(path.string().c_str(), O_RDONLY);
}
void closeFd(int fd) { ::close(fd); }
bool readAt(int fd, u64 offset, u8* data, std::size_t n, std::size_t& got) {
    got = 0;
    while (got < n) {
        ssize_t r = ::pread(fd, data + got, n - got, static_cast<off_t>(offset + got));
        if (r < 0 && errno == EINTR) {
            continue;
        }
        if (r <= 0) {
            break;
        }
        got += static_cast<std::size_t>(r);
    }
    return true;
}
u64 fileLength(int fd) {
    struct stat st;
    if (::fstat(fd, &st) != 0) {
        return 0;
    }
    return static_cast<u64>(st.st_size);
}
bool modeIsReadOnly(const fs::path& path) {
    struct stat st;
    if (::stat(path.string().c_str(), &st) != 0) {
        return false;
    }
    return (st.st_mode & 0222) == 0;
}
int createTempFile(const fs::path& path) {
    return ::open(path.string().c_str(), O_WRONLY | O_CREAT | O_EXCL, 0666);
}
bool writeAll(int fd, const u8* data, std::size_t n) {
    std::size_t done = 0;
    while (done < n) {
        ssize_t r = ::write(fd, data + done, n - done);
        if (r < 0 && errno == EINTR) {
            continue;
        }
        if (r <= 0) {
            return false;
        }
        done += static_cast<std::size_t>(r);
    }
    return true;
}
bool syncFd(int fd) { return ::fsync(fd) == 0; }
#endif

} // namespace

std::unique_ptr<PagedFile> PagedFile::open(const fs::path& path, std::string& error) {
    int fd = openReadOnly(path);
    if (fd < 0) {
        error = strprintf("Error opening file: %s", std::strerror(errno));
        return nullptr;
    }
    auto file = std::unique_ptr<PagedFile>(new PagedFile());
    file->path_ = path;
    file->fd_ = fd;
    file->length_ = fileLength(fd);
    file->readOnly_ = modeIsReadOnly(path);
    return file;
}

PagedFile::~PagedFile() {
    if (fd_ >= 0) {
        closeFd(fd_);
    }
}

const std::vector<u8>& PagedFile::page(u64 pageNumber) {
    auto it = pages_.find(pageNumber);
    if (it != pages_.end()) {
        return it->second;
    }
    u64 start = pageNumber * (64 * 1024);
    std::vector<u8> data;
    if (start < length_) {
        u64 want64 = std::min<u64>(length_ - start, 64 * 1024);
        std::size_t want = static_cast<std::size_t>(want64);
        data.resize(want);
        std::size_t got = 0;
        if (!readAt(fd_, start, data.data(), want, got) || got != want) {
            data.resize(got);
        }
    }
    auto result = pages_.emplace(pageNumber, std::move(data));
    return result.first->second;
}

std::vector<u8> PagedFile::readRange(u64 offset, std::size_t n) {
    if (offset >= length_ || n == 0) {
        return {};
    }
    u64 want = std::min<u64>(length_ - offset, static_cast<u64>(n));
    std::vector<u8> out;
    out.reserve(static_cast<std::size_t>(want));
    u64 pos = offset;
    const u64 end = offset + want;
    while (pos < end) {
        u64 pageNumber = pos / (64 * 1024);
        std::size_t within = static_cast<std::size_t>(pos % (64 * 1024));
        const std::vector<u8>& data = page(pageNumber);
        if (within >= data.size()) {
            break;
        }
        std::size_t available = data.size() - within;
        std::size_t take = static_cast<std::size_t>(
            std::min<u64>(available, end - pos));
        out.insert(out.end(), data.begin() + static_cast<std::ptrdiff_t>(within),
                   data.begin() + static_cast<std::ptrdiff_t>(within + take));
        pos += take;
    }
    auto first = edits_.lower_bound(offset);
    auto last = edits_.lower_bound(end);
    for (auto it = first; it != last; ++it) {
        u64 i = it->first - offset;
        if (i < out.size()) {
            out[static_cast<std::size_t>(i)] = it->second;
        }
    }
    return out;
}

std::optional<u8> PagedFile::readOffset(u64 offset) {
    if (offset >= length_) {
        return std::nullopt;
    }
    auto it = edits_.find(offset);
    if (it != edits_.end()) {
        return it->second;
    }
    u64 pageNumber = offset / (64 * 1024);
    std::size_t within = static_cast<std::size_t>(offset % (64 * 1024));
    const std::vector<u8>& data = page(pageNumber);
    if (within >= data.size()) {
        return std::nullopt;
    }
    return data[within];
}

bool PagedFile::writeOffset(u64 offset, u8 value, std::string& error) {
    if (offset > length_) {
        error = "offset past end of file";
        return false;
    }
    if (offset == length_) {
        edits_[offset] = value;
        ++length_;
    } else {
        auto original = readOffset(offset);
        if (!original) {
            error = "offset past end of file";
            return false;
        }
        if (value == *original) {
            edits_.erase(offset);
        } else {
            edits_[offset] = value;
        }
    }
    dirty_ = !edits_.empty();
    return true;
}

u64 PagedFile::save(std::string& error) {
    if (!dirty_) {
        error = "No changes to write";
        return 0;
    }
    if (readOnly_) {
        error = "File is read-only - can't write";
        return 0;
    }

    fs::path name = path_.filename();
    if (name.empty()) {
        name = "cable.tmp";
    }
    fs::path dir = path_.parent_path();
    if (dir.empty()) {
        dir = ".";
    }
#ifdef _WIN32
    unsigned long pid = GetCurrentProcessId();
#else
    unsigned long pid = static_cast<unsigned long>(::getpid());
#endif
    auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(
                     std::chrono::system_clock::now().time_since_epoch())
                     .count();
    fs::path tmp = dir / strprintf(".%s.cable-cpp-%lu-%lld.tmp", name.string().c_str(), pid,
                                   static_cast<long long>(nanos));

    int src = openReadOnly(path_);
    if (src < 0) {
        error = "Error opening file: could not reopen source";
        return 0;
    }
    int dst = createTempFile(tmp);
    if (dst < 0) {
        closeFd(src);
        error = "Error creating temporary file";
        return 0;
    }

    std::vector<u8> buf(64 * 1024);
    u64 pos = 0;
    u64 written = 0;
    bool ok = true;
    while (pos < length_) {
        std::size_t want = static_cast<std::size_t>(std::min<u64>(length_ - pos, buf.size()));
        std::size_t got = 0;
        if (!readAt(src, pos, buf.data(), want, got)) {
            ok = false;
            break;
        }
        if (got == 0) {
            break;
        }
        auto first = edits_.lower_bound(pos);
        auto last = edits_.lower_bound(pos + got);
        for (auto it = first; it != last; ++it) {
            u64 i = it->first - pos;
            if (i < got) {
                buf[static_cast<std::size_t>(i)] = it->second;
            }
        }
        if (!writeAll(dst, buf.data(), got)) {
            ok = false;
            break;
        }
        written += got;
        pos += got;
    }
    if (ok) {
        auto it = edits_.find(written);
        while (it != edits_.end() && it->first == written) {
            if (!writeAll(dst, &it->second, 1)) {
                ok = false;
                break;
            }
            ++written;
            it = edits_.find(written);
        }
    }
    if (ok && !syncFd(dst)) {
        ok = false;
    }
    closeFd(dst);
    closeFd(src);

    if (!ok) {
        std::error_code ec;
        fs::remove(tmp, ec);
        error = "Error writing file";
        return 0;
    }

#ifdef _WIN32
    {
        std::error_code ec;
        fs::remove(path_, ec);
    }
#endif
    {
        std::error_code ec;
        fs::rename(tmp, path_, ec);
        if (ec) {
            fs::remove(tmp, ec);
            error = "Error renaming temporary file: " + ec.message();
            return 0;
        }
    }

    closeFd(fd_);
    fd_ = openReadOnly(path_);
    if (fd_ < 0) {
        error = "Error reopening file";
        return 0;
    }
    length_ = fileLength(fd_);
    readOnly_ = modeIsReadOnly(path_);
    pages_.clear();
    edits_.clear();
    dirty_ = false;
    return length_;
}

// ---------------------------------------------------------------------------
// Character encodings

std::optional<u8> encodeCP037(char32_t c) {
    for (int i = 0; i < 256; ++i) {
        if (kCP037[i] == c) {
            return static_cast<u8>(i);
        }
    }
    return std::nullopt;
}

char32_t displayByte(u8 b, bool ebcdic, bool conceal) {
    if (ebcdic) {
        char32_t c = kCP037[b];
        auto u = static_cast<std::uint32_t>(c);
        if (u < 0x20 || u == 0x7F) {
            if (conceal) {
                return U'.';
            }
            return kCP437[u & 0xFF];
        }
        return c;
    }
    if (b < 0x20 || b == 0x7F) {
        if (conceal) {
            return U'.';
        }
        return kCP437[b];
    }
    return kCP437[b];
}

static char32_t upperRune(char32_t c) {
    if (c >= U'a' && c <= U'z') {
        return c - 0x20;
    }
    return c;
}

u8 foldByte(u8 b, bool ebcdic) {
    if (ebcdic) {
        char32_t c = upperRune(kCP037[b]);
        if (auto v = encodeCP037(c)) {
            return *v;
        }
        return b;
    }
    if (b >= 'a' && b <= 'z') {
        return static_cast<u8>(b - 0x20);
    }
    return b;
}

bool encodeText(std::string_view text, bool ebcdic, std::vector<u8>& out, std::string& error) {
    out.clear();
    for (char32_t c : utf8Decode(text)) {
        if (ebcdic) {
            auto b = encodeCP037(c);
            if (!b) {
                error = "Character '" + utf8Encode(c) + "' cannot be encoded as EBCDIC";
                return false;
            }
            out.push_back(*b);
        } else {
            if (c > 0xFF) {
                error = "Character '" + utf8Encode(c) + "' cannot be encoded as ASCII";
                return false;
            }
            out.push_back(static_cast<u8>(c));
        }
    }
    return true;
}

static int hexDigit(char32_t c) {
    if (c >= U'0' && c <= U'9') {
        return static_cast<int>(c - U'0');
    }
    if (c >= U'a' && c <= U'f') {
        return static_cast<int>(c - U'a') + 10;
    }
    if (c >= U'A' && c <= U'F') {
        return static_cast<int>(c - U'A') + 10;
    }
    return -1;
}

static bool isSpaceRune(char32_t c) {
    return c == U' ' || c == U'\t' || c == U'\r' || c == U'\n' || c == U'\v' || c == U'\f';
}

bool parseHex(std::string_view text, std::vector<u8>& out, std::string& error) {
    out.clear();
    std::vector<u8> digits;
    for (char32_t c : utf8Decode(text)) {
        if (isSpaceRune(c)) {
            continue;
        }
        int v = hexDigit(c);
        if (v < 0) {
            error = "Invalid HEX digit '" + utf8Encode(c) + "'";
            return false;
        }
        digits.push_back(static_cast<u8>(v));
    }
    if (digits.size() % 2 != 0) {
        error = "HEX string must contain an even number of digits";
        return false;
    }
    out.resize(digits.size() / 2);
    for (std::size_t i = 0; i < out.size(); ++i) {
        out[i] = static_cast<u8>((digits[2 * i] << 4) | digits[2 * i + 1]);
    }
    return true;
}

static std::string trimSpaces(std::string_view text) {
    std::size_t begin = 0;
    std::size_t end = text.size();
    while (begin < end && (text[begin] == ' ' || text[begin] == '\t' || text[begin] == '\r' ||
                           text[begin] == '\n')) {
        ++begin;
    }
    while (end > begin && (text[end - 1] == ' ' || text[end - 1] == '\t' ||
                           text[end - 1] == '\r' || text[end - 1] == '\n')) {
        --end;
    }
    return std::string(text.substr(begin, end - begin));
}

bool parseOffset(std::string_view text, u64& out, std::string& error) {
    std::string t = trimSpaces(text);
    if (t.empty()) {
        error = "No value entered";
        return false;
    }
    int radix = 10;
    std::string digits = t;
    if (t.size() >= 1 && (t[0] == 'H' || t[0] == 'h')) {
        radix = 16;
        digits = t.substr(1);
    } else if (t.size() >= 2 && t[0] == '0' && (t[1] == 'x' || t[1] == 'X')) {
        radix = 16;
        digits = t.substr(2);
    }
    std::string clean;
    for (char c : digits) {
        if (c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == '\v' || c == '\f') {
            continue;
        }
        clean.push_back(c);
    }
    if (clean.empty()) {
        error = "No value entered";
        return false;
    }
    u64 value = 0;
    auto result = std::from_chars(clean.data(), clean.data() + clean.size(), value, radix);
    if (result.ec != std::errc() || result.ptr != clean.data() + clean.size()) {
        error = "Invalid number \"" + t + "\"";
        return false;
    }
    out = value;
    return true;
}

// ---------------------------------------------------------------------------
// Search and compare

static void folded(std::vector<u8>& data, bool ebcdic, bool respectCase) {
    if (respectCase) {
        return;
    }
    for (auto& b : data) {
        b = foldByte(b, ebcdic);
    }
}

std::optional<u64> findBytes(PagedFile& file, const std::vector<u8>& needleIn, u64 start,
                             bool forward, bool respectCase, bool ebcdic) {
    const std::size_t n = needleIn.size();
    if (n == 0) {
        return std::nullopt;
    }
    u64 length = file.length();
    if (length < n) {
        return std::nullopt;
    }
    std::vector<u8> needle = needleIn;
    folded(needle, ebcdic, respectCase);
    const std::size_t chunk = 64 * 1024;

    if (forward) {
        if (start >= length) {
            return std::nullopt;
        }
        u64 pos = start;
        while (pos + n <= length) {
            std::size_t want = chunk + n - 1;
            std::vector<u8> buf = file.readRange(pos, want);
            if (buf.size() < n) {
                break;
            }
            folded(buf, ebcdic, respectCase);
            std::size_t last = buf.size() - n;
            for (std::size_t i = 0; i <= last; ++i) {
                if (std::equal(needle.begin(), needle.end(), buf.begin() + i)) {
                    return pos + i;
                }
            }
            if (pos + chunk >= length) {
                break;
            }
            pos += chunk;
        }
        return std::nullopt;
    }

    u64 maxStart = std::min<u64>(start, length - n);
    u64 high = maxStart;
    for (;;) {
        u64 low = high > chunk ? high - chunk : 0;
        u64 spanEnd = std::min<u64>(high + n, length);
        std::vector<u8> buf = file.readRange(low, static_cast<std::size_t>(spanEnd - low));
        if (buf.size() >= n) {
            folded(buf, ebcdic, respectCase);
            std::size_t maxI = static_cast<std::size_t>(high - low);
            std::size_t last = buf.size() - n;
            if (maxI > last) {
                maxI = last;
            }
            for (std::size_t i = maxI;; --i) {
                if (std::equal(needle.begin(), needle.end(), buf.begin() + i)) {
                    return low + i;
                }
                if (i == 0) {
                    break;
                }
            }
        }
        if (low == 0) {
            break;
        }
        high = low - 1;
    }
    return std::nullopt;
}

std::optional<u64> findDifference(PagedFile& a, PagedFile& b, u64 start, bool forward) {
    u64 minLen = std::min(a.length(), b.length());
    const u64 chunk = 64 * 1024;

    if (forward) {
        u64 pos = start;
        while (pos < minLen) {
            u64 want = std::min<u64>(minLen - pos, chunk);
            std::vector<u8> left = a.readRange(pos, static_cast<std::size_t>(want));
            std::vector<u8> right = b.readRange(pos, static_cast<std::size_t>(want));
            std::size_t n = std::min(left.size(), right.size());
            for (std::size_t i = 0; i < n; ++i) {
                if (left[i] != right[i]) {
                    return pos + i;
                }
            }
            pos += want;
        }
        if (a.length() != b.length() && start <= minLen) {
            return minLen;
        }
        return std::nullopt;
    }

    u64 limit = start;
    if (minLen == 0) {
        limit = 0;
    } else if (limit > minLen - 1) {
        limit = minLen - 1;
    }
    u64 pos = 0;
    u64 lastValue = 0;
    bool found = false;
    while (pos <= limit) {
        u64 want = std::min<u64>(limit - pos + 1, chunk);
        std::vector<u8> left = a.readRange(pos, static_cast<std::size_t>(want));
        std::vector<u8> right = b.readRange(pos, static_cast<std::size_t>(want));
        std::size_t n = std::min(left.size(), right.size());
        for (std::size_t i = 0; i < n; ++i) {
            if (left[i] != right[i]) {
                lastValue = pos + i;
                found = true;
            }
        }
        pos += want;
        if (want == 0) {
            break;
        }
    }
    if (a.length() != b.length() && minLen <= start) {
        lastValue = minLen;
        found = true;
    }
    if (!found) {
        return std::nullopt;
    }
    return lastValue;
}

// ---------------------------------------------------------------------------
// Disassembly

const char* cpuLabel(Cpu cpu) {
    switch (cpu) {
    case Cpu::X86:
        return "x86";
    case Cpu::X64:
        return "x64";
    case Cpu::Arm:
        return "ARM";
    case Cpu::Thumb:
        return "Thumb";
    default:
        return "ARM64";
    }
}

std::optional<Cpu> parseCpu(std::string_view name) {
    std::string lower;
    for (char c : name) {
        lower.push_back(static_cast<char>(std::tolower(static_cast<unsigned char>(c))));
    }
    if (lower == "x86" || lower == "i386" || lower == "32") {
        return Cpu::X86;
    }
    if (lower == "x64" || lower == "x86-64" || lower == "amd64" || lower == "64") {
        return Cpu::X64;
    }
    if (lower == "arm" || lower == "armv7") {
        return Cpu::Arm;
    }
    if (lower == "thumb" || lower == "thumb2") {
        return Cpu::Thumb;
    }
    if (lower == "arm64" || lower == "aarch64" || lower == "armv8") {
        return Cpu::Arm64;
    }
    return std::nullopt;
}

std::optional<Cpu> nextView(Cpu cpu) {
    switch (cpu) {
    case Cpu::X86:
        return Cpu::X64;
    case Cpu::X64:
        return Cpu::Arm;
    case Cpu::Arm:
        return Cpu::Thumb;
    case Cpu::Thumb:
        return Cpu::Arm64;
    default:
        return std::nullopt;
    }
}

static Insn rawDirective(const std::vector<u8>& rest, std::size_t pos, bool thumb) {
    Insn insn;
    insn.offset = static_cast<int>(pos);
    if (thumb && rest.size() >= 2) {
        unsigned int word = static_cast<unsigned int>(rest[0]) |
                            (static_cast<unsigned int>(rest[1]) << 8);
        insn.len = 2;
        insn.text = strprintf(".hword 0x%04X", word);
        return insn;
    }
    if (!thumb && rest.size() >= 4) {
        unsigned int word = static_cast<unsigned int>(rest[0]) |
                            (static_cast<unsigned int>(rest[1]) << 8) |
                            (static_cast<unsigned int>(rest[2]) << 16) |
                            (static_cast<unsigned int>(rest[3]) << 24);
        insn.len = 4;
        insn.text = strprintf(".word 0x%08X", word);
        return insn;
    }
    std::string text = ".byte";
    std::size_t used = 0;
    for (u8 b : rest) {
        if (used == 4) {
            break;
        }
        text += strprintf(" %02X", b);
        ++used;
    }
    std::size_t length = std::min<std::size_t>(rest.size(), 4);
    insn.len = static_cast<int>(std::max<std::size_t>(length, 1));
    insn.text = text;
    return insn;
}

std::vector<Insn> disassemble(Cpu cpu, const std::vector<u8>& bytes) {
    std::vector<Insn> out;
    std::size_t pos = 0;
    switch (cpu) {
    case Cpu::X86:
    case Cpu::X64:
        while (pos < bytes.size()) {
            out.push_back(Insn{static_cast<int>(pos), 1,
                               strprintf(".byte %02X", bytes[pos])});
            ++pos;
        }
        break;
    case Cpu::Thumb:
        while (pos < bytes.size()) {
            std::vector<u8> rest(bytes.begin() + static_cast<std::ptrdiff_t>(pos), bytes.end());
            Insn insn = rawDirective(rest, pos, true);
            pos += static_cast<std::size_t>(insn.len);
            out.push_back(std::move(insn));
        }
        break;
    case Cpu::Arm:
    case Cpu::Arm64:
        while (pos < bytes.size()) {
            std::vector<u8> rest(bytes.begin() + static_cast<std::ptrdiff_t>(pos), bytes.end());
            Insn insn = rawDirective(rest, pos, false);
            pos += static_cast<std::size_t>(insn.len);
            out.push_back(std::move(insn));
        }
        break;
    }
    return out;
}

// ---------------------------------------------------------------------------
// Directory browser

const char* sortLabel(SortKey key) {
    switch (key) {
    case SortKey::Unordered:
        return "Unordered";
    case SortKey::Name:
        return "by Name";
    case SortKey::Ext:
        return "by Ext";
    case SortKey::Date:
        return "by Date";
    default:
        return "by Size";
    }
}

static std::string lowerAscii(std::string_view text) {
    std::string out;
    out.reserve(text.size());
    for (unsigned char c : text) {
        if (c >= 'A' && c <= 'Z') {
            out.push_back(static_cast<char>(c - 'A' + 'a'));
        } else {
            out.push_back(static_cast<char>(c));
        }
    }
    return out;
}

static std::string extensionOf(const std::string& name) {
    std::string ext = fs::path(name).extension().string();
    if (!ext.empty() && ext[0] == '.') {
        ext.erase(ext.begin());
    }
    return lowerAscii(ext);
}

std::string formatSize(u64 n) {
    if (n < 1024) {
        return strprintf("%llu", static_cast<unsigned long long>(n));
    }
    if (n < 1024ULL * 1024) {
        return strprintf("%.1fK", static_cast<double>(n) / 1024.0);
    }
    if (n < 1024ULL * 1024 * 1024) {
        return strprintf("%.1fM", static_cast<double>(n) / (1024.0 * 1024.0));
    }
    return strprintf("%.1fG", static_cast<double>(n) / (1024.0 * 1024.0 * 1024.0));
}

std::string formatTime(std::time_t t, bool ok) {
    if (!ok) {
        return {};
    }
    std::tm tm{};
#ifdef _WIN32
    gmtime_s(&tm, &t);
#else
    gmtime_r(&t, &tm);
#endif
    return strprintf("%04d-%02d-%02d %02d:%02d", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday,
                     tm.tm_hour, tm.tm_min);
}

static std::time_t fileTimeToTimeT(fs::file_time_type ft) {
    using namespace std::chrono;
    auto systemTime = time_point_cast<system_clock::duration>(
        ft - fs::file_time_type::clock::now() + system_clock::now());
    return system_clock::to_time_t(systemTime);
}

static std::string absolutePath(const fs::path& path) {
    if (path.is_absolute()) {
        return path.string();
    }
    std::error_code ec;
    fs::path cwd = fs::current_path(ec);
    if (ec) {
        return path.string();
    }
    return (cwd / path).string();
}

static std::string expandHome(std::string path) {
    if (!path.empty() && path[0] == '~' &&
        (path.size() == 1 || path[1] == '/' || path[1] == '\\')) {
        const char* home = std::getenv("HOME");
        if (home == nullptr || *home == '\0') {
#ifdef _WIN32
            home = std::getenv("USERPROFILE");
#endif
        }
        if (home != nullptr && *home != '\0') {
            return (fs::path(home) / path.substr(1)).string();
        }
    }
    return path;
}

void sortEntries(std::vector<DirEntry>& entries, SortKey key) {
    auto dirFirst = [](const DirEntry& a, const DirEntry& b) {
        if (a.isDir != b.isDir) {
            return a.isDir;
        }
        return false;
    };
    switch (key) {
    case SortKey::Unordered:
        return;
    case SortKey::Name:
        std::stable_sort(entries.begin(), entries.end(), [&](const DirEntry& a, const DirEntry& b) {
            if (a.isDir != b.isDir) {
                return a.isDir;
            }
            std::string an = lowerAscii(a.name);
            std::string bn = lowerAscii(b.name);
            if (an != bn) {
                return an < bn;
            }
            return a.name < b.name;
        });
        break;
    case SortKey::Ext:
        std::stable_sort(entries.begin(), entries.end(), [&](const DirEntry& a, const DirEntry& b) {
            if (a.isDir != b.isDir) {
                return a.isDir;
            }
            std::string ea = extensionOf(a.name);
            std::string eb = extensionOf(b.name);
            if (ea != eb) {
                return ea < eb;
            }
            return lowerAscii(a.name) < lowerAscii(b.name);
        });
        break;
    case SortKey::Date:
        std::stable_sort(entries.begin(), entries.end(), [&](const DirEntry& a, const DirEntry& b) {
            if (a.isDir != b.isDir) {
                return a.isDir;
            }
            if (a.modified != b.modified) {
                return a.modified > b.modified;
            }
            return lowerAscii(a.name) < lowerAscii(b.name);
        });
        break;
    case SortKey::Size:
        std::stable_sort(entries.begin(), entries.end(), [&](const DirEntry& a, const DirEntry& b) {
            if (a.isDir != b.isDir) {
                return a.isDir;
            }
            if (a.size != b.size) {
                return a.size > b.size;
            }
            return lowerAscii(a.name) < lowerAscii(b.name);
        });
        break;
    }
    (void)dirFirst;
}

DirBrowser::DirBrowser(fs::path dir, int targetPane) {
    dir_ = absolutePath(dir);
    targetPane_ = targetPane;
    sort_ = SortKey::Unordered;
    refresh();
}

void DirBrowser::refresh() {
    error_.clear();
    std::vector<DirEntry> entries;
    std::error_code ec;
    fs::directory_iterator it(dir_, ec);
    if (ec) {
        error_ = "Error opening file: " + ec.message();
    } else {
        fs::directory_iterator end;
        for (; it != end; it.increment(ec)) {
            if (ec) {
                error_ = "Error opening file: " + ec.message();
                break;
            }
            std::string name = it->path().filename().string();
            if (name == "." || name == "..") {
                continue;
            }
            DirEntry entry;
            entry.name = name;
            std::error_code sizeEc;
            auto size = it->file_size(sizeEc);
            if (!sizeEc) {
                entry.size = static_cast<u64>(size);
            }
            std::error_code timeEc;
            auto modified = it->last_write_time(timeEc);
            if (!timeEc) {
                entry.modified = fileTimeToTimeT(modified);
                entry.hasTime = true;
            }
            std::error_code dirEc;
            entry.isDir = it->is_directory(dirEc);
            entries.push_back(std::move(entry));
        }
    }
    sortEntries(entries, sort_);
    entries_ = std::move(entries);
    if (selected_ >= static_cast<int>(entries_.size())) {
        selected_ = static_cast<int>(entries_.size()) - 1;
        if (selected_ < 0) {
            selected_ = 0;
        }
    }
    if (scroll_ > selected_) {
        scroll_ = selected_;
    }
}

const DirEntry* DirBrowser::current() const {
    if (selected_ < 0 || selected_ >= static_cast<int>(entries_.size())) {
        return nullptr;
    }
    return &entries_[static_cast<std::size_t>(selected_)];
}

std::optional<fs::path> DirBrowser::currentPath() const {
    const DirEntry* entry = current();
    if (entry == nullptr) {
        return std::nullopt;
    }
    return fs::path(dir_) / entry->name;
}

void DirBrowser::setSort(SortKey key) {
    sort_ = key;
    sortEntries(entries_, key);
    if (entries_.empty()) {
        selected_ = 0;
    } else if (selected_ >= static_cast<int>(entries_.size())) {
        selected_ = static_cast<int>(entries_.size()) - 1;
    }
    sortPrompt_ = false;
}

void DirBrowser::moveBy(int delta) {
    if (entries_.empty()) {
        selected_ = 0;
        return;
    }
    int max = static_cast<int>(entries_.size()) - 1;
    selected_ += delta;
    if (selected_ < 0) {
        selected_ = 0;
    }
    if (selected_ > max) {
        selected_ = max;
    }
}

void DirBrowser::home() { selected_ = 0; }

void DirBrowser::end() {
    selected_ = entries_.empty() ? 0 : static_cast<int>(entries_.size()) - 1;
}

void DirBrowser::ensureVisible(int page) {
    if (page < 1) {
        page = 1;
    }
    if (selected_ < scroll_) {
        scroll_ = selected_;
    } else if (selected_ >= scroll_ + page) {
        scroll_ = selected_ + 1 - page;
    }
}

std::optional<fs::path> DirBrowser::activate() {
    const DirEntry* entry = current();
    if (entry == nullptr) {
        return std::nullopt;
    }
    fs::path path = fs::path(dir_) / entry->name;
    if (entry->isDir) {
        dir_ = path;
        selected_ = 0;
        scroll_ = 0;
        refresh();
        return std::nullopt;
    }
    return path;
}

void DirBrowser::parent() {
    fs::path parentPath = dir_.parent_path();
    if (parentPath != dir_) {
        dir_ = parentPath;
        selected_ = 0;
        scroll_ = 0;
        refresh();
    }
}

void DirBrowser::toggleTarget() { targetPane_ = 1 - targetPane_; }

void DirBrowser::beginPathInput() {
    std::string text = dir_.string();
#ifdef _WIN32
    const char sep = '\\';
#else
    const char sep = '/';
#endif
    if (text.empty() || text.back() != sep) {
        text.push_back(sep);
    }
    pathInput_ = text;
    error_.clear();
}

void DirBrowser::applyPathInput() {
    if (!pathInput_) {
        return;
    }
    std::string raw = *pathInput_;
    pathInput_.reset();
    std::string spec = expandHome(trimSpaces(raw));
    if (spec.empty()) {
        return;
    }
    fs::path candidate;
    if (fs::path(spec).is_absolute()) {
        candidate = spec;
    } else {
        candidate = fs::path(dir_) / spec;
    }
    std::error_code ec;
    if (fs::is_directory(candidate, ec)) {
        dir_ = candidate;
        selected_ = 0;
        scroll_ = 0;
        refresh();
    } else if (fs::is_regular_file(candidate, ec)) {
        fs::path parentPath = candidate.parent_path();
        std::string name = candidate.filename().string();
        if (parentPath != dir_) {
            dir_ = parentPath;
            selected_ = 0;
            scroll_ = 0;
            refresh();
        }
        bool found = false;
        for (std::size_t i = 0; i < entries_.size(); ++i) {
            if (entries_[i].name == name) {
                selected_ = static_cast<int>(i);
                found = true;
                break;
            }
        }
        if (!found) {
            error_ = "No matching files found in directory: " + name;
        }
    } else {
        error_ = "Error opening file: " + candidate.string();
    }
}
