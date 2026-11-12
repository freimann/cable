// CABLE-CPP: a cross-platform reimplementation of CABLE V1.7X.
//
// This is a direct C++ port of the Rust cable-rs project.  The file I/O layer
// keeps the same 64 KiB paged reads and edit overlay so its performance can be
// compared with the Rust and Go versions.

#pragma once

#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <deque>
#include <filesystem>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

using u8 = std::uint8_t;
using u64 = std::uint64_t;
namespace fs = std::filesystem;

// ---------------------------------------------------------------------------
// UTF-8 helpers

std::vector<char32_t> utf8Decode(std::string_view text);
std::string utf8Encode(char32_t c);
std::size_t runeLen(std::string_view text);
std::string truncateRunes(std::string_view text, std::size_t width);
std::string padRight(std::string_view text, std::size_t width);
std::string padLeft(std::string_view text, std::size_t width);
std::string collapseSpaces(std::string_view text);

template <typename... Args>
std::string strprintf(const char* format, Args... args) {
    int n = std::snprintf(nullptr, 0, format, args...);
    if (n <= 0) {
        return {};
    }
    std::vector<char> buffer(static_cast<std::size_t>(n) + 1);
    std::snprintf(buffer.data(), buffer.size(), format, args...);
    return std::string(buffer.data(), static_cast<std::size_t>(n));
}

// ---------------------------------------------------------------------------
// Styles and screen

struct Style {
    std::string fg;
    std::string bg;
    bool bold = false;

    bool operator==(const Style& other) const {
        return fg == other.fg && bg == other.bg && bold == other.bold;
    }
    bool operator!=(const Style& other) const { return !(*this == other); }
};

std::string fg256(int n);
std::string bg256(int n);
std::string fgRGB(int r, int g, int b);
std::string bgRGB(int r, int g, int b);

struct Rect {
    int x = 0;
    int y = 0;
    int w = 0;
    int h = 0;
};

struct Cell {
    char32_t ch = U' ';
    Style style;
};

struct Screen {
    int w = 0;
    int h = 0;
    std::vector<Cell> cells;

    Screen(int width, int height, const Style& base);
    void set(int x, int y, char32_t ch, const Style& style);
    int put(int x, int y, std::string_view text, const Style& style);
    int putLimit(int x, int y, std::string_view text, const Style& style, int limit);
    void restyle(const Rect& rect, const Style& style);
    void fill(const Rect& rect, char32_t ch, const Style& style);
    void flush(std::string& out) const;
};

std::vector<std::string> wrapText(std::string_view text, int width);
int satSub(int a, int b);
int maxInt(int a, int b);

// ---------------------------------------------------------------------------
// Paged file

class PagedFile {
public:
    static std::unique_ptr<PagedFile> open(const fs::path& path, std::string& error);
    ~PagedFile();

    PagedFile(const PagedFile&) = delete;
    PagedFile& operator=(const PagedFile&) = delete;

    const fs::path& path() const { return path_; }
    u64 length() const { return length_; }
    bool readOnly() const { return readOnly_; }
    bool dirty() const { return dirty_; }

    std::vector<u8> readRange(u64 offset, std::size_t n);
    std::optional<u8> readOffset(u64 offset);
    bool writeOffset(u64 offset, u8 value, std::string& error);
    u64 save(std::string& error);

private:
    PagedFile() = default;
    const std::vector<u8>& page(u64 page);

    fs::path path_;
    int fd_ = -1;
    u64 length_ = 0;
    bool readOnly_ = false;
    bool dirty_ = false;
    std::unordered_map<u64, std::vector<u8>> pages_;
    std::map<u64, u8> edits_;
};

// ---------------------------------------------------------------------------
// Character encodings

std::optional<u8> encodeCP037(char32_t c);
char32_t displayByte(u8 b, bool ebcdic, bool conceal);
u8 foldByte(u8 b, bool ebcdic);
bool encodeText(std::string_view text, bool ebcdic, std::vector<u8>& out, std::string& error);
bool parseHex(std::string_view text, std::vector<u8>& out, std::string& error);
bool parseOffset(std::string_view text, u64& out, std::string& error);

// ---------------------------------------------------------------------------
// Search and compare

std::optional<u64> findBytes(PagedFile& file, const std::vector<u8>& needle, u64 start,
                             bool forward, bool respectCase, bool ebcdic);
std::optional<u64> findDifference(PagedFile& a, PagedFile& b, u64 start, bool forward);

// ---------------------------------------------------------------------------
// Disassembly

enum class Cpu { X86, X64, Arm, Thumb, Arm64 };

const char* cpuLabel(Cpu cpu);
std::optional<Cpu> parseCpu(std::string_view name);
std::optional<Cpu> nextView(Cpu cpu);

struct Insn {
    int offset = 0;
    int len = 0;
    std::string text;
};

std::vector<Insn> disassemble(Cpu cpu, const std::vector<u8>& bytes);

// ---------------------------------------------------------------------------
// Directory browser

enum class SortKey { Unordered, Name, Ext, Date, Size };

const char* sortLabel(SortKey key);

struct DirEntry {
    std::string name;
    u64 size = 0;
    std::time_t modified = 0;
    bool hasTime = false;
    bool isDir = false;
};

std::string formatSize(u64 n);
std::string formatTime(std::time_t t, bool ok);

class DirBrowser {
public:
    DirBrowser(fs::path dir, int targetPane);

    const fs::path& dir() const { return dir_; }
    const std::vector<DirEntry>& entries() const { return entries_; }
    int selected() const { return selected_; }
    int scroll() const { return scroll_; }
    SortKey sort() const { return sort_; }
    int targetPane() const { return targetPane_; }
    bool sortPrompt() const { return sortPrompt_; }
    const std::optional<std::string>& pathInput() const { return pathInput_; }
    const std::string& error() const { return error_; }

    void refresh();
    void setSort(SortKey key);
    void moveBy(int delta);
    void home();
    void end();
    void ensureVisible(int page);
    std::optional<fs::path> currentPath() const;
    // Returns a file path when a file was activated; navigating into a
    // directory returns nullopt and keeps the browser open.
    std::optional<fs::path> activate();
    void parent();
    void toggleTarget();
    void beginPathInput();
    void applyPathInput();

    void setSortPrompt(bool value) { sortPrompt_ = value; }
    void setPathInput(std::optional<std::string> value) { pathInput_ = std::move(value); }

private:
    const DirEntry* current() const;
    fs::path dir_;
    std::vector<DirEntry> entries_;
    int selected_ = 0;
    int scroll_ = 0;
    SortKey sort_ = SortKey::Unordered;
    int targetPane_ = 0;
    bool sortPrompt_ = false;
    std::optional<std::string> pathInput_;
    std::string error_;
};

void sortEntries(std::vector<DirEntry>& entries, SortKey key);

// ---------------------------------------------------------------------------
// Keyboard

enum class KeyCode {
    Rune, Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Delete,
    Enter, Esc, Backspace, Tab, BackTab, F1, F2, F3, F4, F5, F6, F7, F8, F9,
    F10, F11, F12, ScrollLock
};

struct Key {
    KeyCode code = KeyCode::Rune;
    char32_t rune = 0;
    bool ctrl = false;
    bool alt = false;
    bool shift = false;

    bool isF(int n) const {
        return static_cast<int>(code) == static_cast<int>(KeyCode::F1) + n - 1;
    }
};

bool isScrollLockToggle(const Key& key);

class KeyReader {
public:
    std::optional<Key> poll(int timeoutMs);

private:
    bool readByte(int timeoutMs, unsigned char& out);
    std::optional<Key> readKey();
    std::optional<Key> parseFirst(unsigned char b);
    Key decodeByte(unsigned char b);
    std::optional<Key> readCSI();
    std::optional<Key> readSS3();
    std::deque<Key> pending_;
};

// ---------------------------------------------------------------------------
// Terminal

class Terminal {
public:
    Terminal() = default;
    ~Terminal();
    Terminal(const Terminal&) = delete;
    Terminal& operator=(const Terminal&) = delete;

    static std::unique_ptr<Terminal> start(std::string& error);
    void close();

private:
    bool active_ = false;
};

bool terminalSize(int& width, int& height);

// Writes terminal output.  On Windows this routes through the ANSI-to-console
// translator when the console lacks virtual-terminal support (Windows 7/8).
void writeTerminal(const std::string& text);

// ---------------------------------------------------------------------------
// Application

enum class Theme { Classic, Amber, Mono };

std::string themeName(Theme theme);
std::string themeBG(Theme theme);
std::string themeFG(Theme theme);
std::string themeDim(Theme theme);
std::string themeBarBG(Theme theme);
std::string themeBarFG(Theme theme);
std::string themeAccent(Theme theme);
std::string themeHeaderFG(Theme theme);
std::string themeCursorFG(Theme theme);
std::string themeCursorBG(Theme theme);
std::string themeDiffFG(Theme theme);
std::string themeDiffBG(Theme theme);
std::string themeBorder(Theme theme);

struct MenuItem {
    const char* label;
    char32_t key;
};

extern const std::array<MenuItem, 14> kMenuItems;

inline Style baseStyle(Theme theme) { return Style{themeFG(theme), themeBG(theme), false}; }
inline Style barStyle(Theme theme) { return Style{themeBarFG(theme), themeBarBG(theme), false}; }

struct Options {
    bool ebcdic = false;
    bool conceal = true;
    bool respectCase = true;
    bool forward = true;
    bool hexInput = false;
    bool moveBoth = false;
    Theme theme = Theme::Classic;
    std::optional<Cpu> disasm;

    std::string charset() const { return ebcdic ? "EBCDIC" : "ASCII"; }
    std::string direction() const { return forward ? "Forward" : "Backward"; }
    std::string caseLabel() const { return respectCase ? "Respect" : "Ignore"; }
    std::string show() const { return conceal ? "Conceal" : "Show"; }
    std::string input() const { return hexInput ? "HEX" : "Character"; }
    std::string scrollLock() const { return moveBoth ? "Both" : "Active"; }
    std::string sideView() const { return disasm ? cpuLabel(*disasm) : charset(); }
    void cycleSideView();
};

Options defaultOptions();

struct Pane {
    std::unique_ptr<PagedFile> file;
    u64 cursor = 0;
    u64 top = 0;

    u64 length() const { return file ? file->length() : 0; }
    std::string name() const;
    bool dirty() const { return file && file->dirty(); }
    u64 cursorMax(bool allowAppend) const;
    void clampCursor(bool allowAppend);
    void ensureVisible(int rows, int bytesPerRow);
};

enum class ModalKind { Confirm, Input, Options, Modes, Attributes, Help, Dir };
enum class ConfirmAction { Quit, Write, Load };
enum class InputAction { Add, Sub, Goto, Search, View };

struct Modal {
    ModalKind kind = ModalKind::Confirm;
    std::string title;
    std::string text;
    std::string prompt;
    std::string value;
    InputAction input = InputAction::Add;
    ConfirmAction confirm = ConfirmAction::Quit;
    std::string path;
    int pane = 0;
    int index = 0;
    int scroll = 0;
    std::unique_ptr<DirBrowser> dir;
};

struct EditState {
    u8 nibble = 0;
    bool has = false;
};

enum class LastActionKind { Search, Compare };

struct LastAction {
    LastActionKind kind = LastActionKind::Search;
    std::vector<u8> needle;
};

class App {
public:
    static std::unique_ptr<App> create(const std::vector<fs::path>& paths, Options options,
                                       std::string& error);

    std::array<Pane, 2> panes;
    int active = 0;
    Options options;
    std::unique_ptr<Modal> modal;
    int menu = 3;
    bool quit = false;
    std::string message;
    bool hasMsg = false;
    int rowsPerPane = 10;
    int bytesPerRow = 16;
    bool compareMode = false;
    std::unique_ptr<LastAction> last;
    std::unique_ptr<EditState> edit;

    void handleKey(const Key& key);
    void syncGeometry(int rows, int bytesPerRow);
    std::string statusLine() const;
    std::string fkeyLine() const;
    std::string menuDescription() const;

private:
    Pane& activePane() { return panes[active]; }
    const Pane& activePane() const { return panes[active]; }
    bool anyDirty() const { return panes[0].dirty() || panes[1].dirty(); }
    bool bothPanes() const;
    void setMessage(std::string text);
    void clearMessage();
    void switchPane();
    void moveCursor(long long delta);
    void moveHome();
    void moveEnd();
    void handleCommandChar(char32_t c);
    void executeMenu(int index);
    void openHelp();
    void openOptions();
    void openModes();
    void openDir();
    void openInput(std::string title, std::string prompt, InputAction action, std::string value);
    void openOffsetInput(InputAction action);
    void openSearch();
    void openView();
    std::string editPrompt() const;
    void startEdit();
    void handleEditKey(const Key& key);
    void editByte(u8 value);
    void startCompare();
    void doCompare(bool advance);
    void repeatLast();
    void performSearch(const std::vector<u8>& needle, bool advance);
    void writeConfirm();
    void doWrite();
    void tryQuit();
    void loadInto(fs::path path, int pane);
    void loadIntoForce(fs::path path, int pane);
    void submitInput(InputAction action, const std::string& value);
    void handleModalKey(const Key& key);
    void handleDirKey(Modal& modal, const Key& key);
    void activateOption(int index);
    void optionLetter(char32_t c);
    void activateMode(int index);
};

// ---------------------------------------------------------------------------
// Rendering

void drawUI(Screen& screen, App& app);
void drawModal(Screen& screen, Modal& modal, Options& options, Theme theme, const Rect& area);
void drawDir(Screen& screen, DirBrowser& browser, Theme theme, const Rect& area);
int adaptiveBytesPerRow(int width);
int adaptiveHexCenter(int bytesPerRow, int dataCol);
std::vector<std::vector<Cell>> disasmWindow(App& app, int index, int rows, int width);

// ---------------------------------------------------------------------------
// Benchmark

int runBenchmark(const std::vector<fs::path>& paths);

extern const char* kHelpTitle;
