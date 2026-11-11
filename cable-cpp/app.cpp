// Application state and key handling.

#include "cable.hpp"
#include "help_text.hpp"

#include <cerrno>

const char* kHelpTitle = "CABLE V1.7X - by David Mitchell (ZENO at WINVMB)";

const std::array<MenuItem, 14> kMenuItems = {{
    {"Compare", U'C'},
    {"Dir", U'D'},
    {"Edit", U'E'},
    {"Help", U'H'},
    {"Mode", U'M'},
    {"Next", U'N'},
    {"Options", U'O'},
    {"+", U'+'},
    {"-", U'-'},
    {"=", U'='},
    {"Quit", U'Q'},
    {"Search", U'S'},
    {"View", U'V'},
    {"Write", U'W'},
}};

std::string themeName(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return "Colour (blue/yellow)";
    case Theme::Amber:
        return "Amber";
    default:
        return "Mono";
    }
}

std::string themeBG(Theme theme) {
    return theme == Theme::Classic ? bg256(4) : bg256(0);
}

std::string themeFG(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return fg256(11);
    case Theme::Amber:
        return fgRGB(255, 191, 0);
    default:
        return fg256(15);
    }
}

std::string themeDim(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return fg256(7);
    case Theme::Amber:
        return fgRGB(150, 100, 0);
    default:
        return fg256(8);
    }
}

std::string themeBarBG(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return bg256(7);
    case Theme::Amber:
        return bgRGB(150, 100, 0);
    default:
        return bg256(8);
    }
}

std::string themeBarFG(Theme theme) {
    return theme == Theme::Classic ? fg256(8) : fg256(0);
}

std::string themeAccent(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return fg256(6);
    case Theme::Amber:
        return fgRGB(255, 220, 120);
    default:
        return fg256(15);
    }
}

std::string themeHeaderFG(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return fg256(15);
    case Theme::Amber:
        return fgRGB(255, 220, 120);
    default:
        return fg256(15);
    }
}

std::string themeCursorFG(Theme theme) {
    return theme == Theme::Classic ? fg256(4) : fg256(0);
}

std::string themeCursorBG(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return bg256(15);
    case Theme::Amber:
        return bgRGB(255, 191, 0);
    default:
        return bg256(15);
    }
}

std::string themeDiffFG(Theme theme) {
    return theme == Theme::Classic ? fg256(15) : fg256(0);
}

std::string themeDiffBG(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return bg256(1);
    case Theme::Amber:
        return bgRGB(255, 191, 0);
    default:
        return bg256(15);
    }
}

std::string themeBorder(Theme theme) {
    switch (theme) {
    case Theme::Classic:
        return fg256(6);
    case Theme::Amber:
        return fgRGB(255, 191, 0);
    default:
        return fg256(15);
    }
}

void Options::cycleSideView() {
    if (!disasm) {
        disasm = Cpu::X86;
        return;
    }
    if (auto next = nextView(*disasm)) {
        disasm = *next;
        return;
    }
    disasm.reset();
}

Options defaultOptions() {
    Options options;
    options.conceal = true;
    options.respectCase = true;
    options.forward = true;
    options.theme = Theme::Classic;
    return options;
}

std::string Pane::name() const {
    if (!file) {
        return "--- empty ---";
    }
    std::string full = file->path().string();
    if (runeLen(full) <= 34) {
        return full;
    }
    return file->path().filename().string();
}

u64 Pane::cursorMax(bool allowAppend) const {
    u64 len = length();
    if (len == 0) {
        return 0;
    }
    return allowAppend ? len : len - 1;
}

void Pane::clampCursor(bool allowAppend) {
    u64 max = cursorMax(allowAppend);
    if (cursor > max) {
        cursor = max;
    }
}

void Pane::ensureVisible(int rows, int bytesPerRow) {
    if (rows < 1) {
        rows = 1;
    }
    if (bytesPerRow < 16) {
        bytesPerRow = 16;
    }
    u64 bpr = static_cast<u64>(bytesPerRow);
    top -= top % bpr;
    u64 page = static_cast<u64>(rows) * bpr;
    u64 lineStart = (cursor / bpr) * bpr;
    if (lineStart < top) {
        top = lineStart;
    } else if (cursor >= top + page) {
        u64 lastRow = cursor / bpr;
        if (lastRow >= static_cast<u64>(rows) - 1) {
            lastRow -= static_cast<u64>(rows) - 1;
        } else {
            lastRow = 0;
        }
        top = lastRow * bpr;
    }
    u64 len = length();
    if (len > 0 && top >= len) {
        top = ((len - 1) / bpr) * bpr;
    }
}

std::unique_ptr<App> App::create(const std::vector<fs::path>& paths, Options options,
                                 std::string& error) {
    auto app = std::unique_ptr<App>(new App());
    app->options = options;
    app->menu = 3;
    app->rowsPerPane = 10;
    app->bytesPerRow = 16;
    for (std::size_t i = 0; i < paths.size() && i < 2; ++i) {
        std::string openError;
        auto file = PagedFile::open(paths[i], openError);
        if (!file) {
            error = "Error opening file " + paths[i].string() + ": " + openError;
            return nullptr;
        }
        app->panes[i].file = std::move(file);
    }
    if (paths.empty()) {
        std::error_code ec;
        fs::path cwd = fs::current_path(ec);
        if (ec) {
            cwd = ".";
        }
        app->modal = std::make_unique<Modal>();
        app->modal->kind = ModalKind::Dir;
        app->modal->dir = std::make_unique<DirBrowser>(cwd, 0);
    }
    return app;
}

bool App::bothPanes() const {
    return options.moveBoth && panes[0].file && panes[1].file;
}

void App::setMessage(std::string text) {
    message = std::move(text);
    hasMsg = true;
}

void App::clearMessage() {
    message.clear();
    hasMsg = false;
}

void App::syncGeometry(int rows, int bytesPerRow) {
    if (rows < 1) {
        rows = 1;
    }
    if (bytesPerRow < 16) {
        bytesPerRow = 16;
    }
    rowsPerPane = rows;
    this->bytesPerRow = bytesPerRow;
    for (auto& pane : panes) {
        if (pane.file) {
            pane.ensureVisible(rows, bytesPerRow);
        }
    }
}

std::string App::statusLine() const {
    if (hasMsg) {
        return message;
    }
    std::string s = options.sideView() + "  " + options.direction() + "  " +
                    options.caseLabel() + "  " + options.show() + "  " + options.input() +
                    "  SL:" + options.scrollLock();
    if (compareMode) {
        s += "  [COMPARE]";
    }
    if (edit) {
        s += "  [EDIT]";
    }
    return s;
}

std::string App::fkeyLine() const {
    return "F1:Help  F2:" + options.caseLabel() + "  F3:" + options.direction() +
           "  F4:" + options.input() + "  F5:" + options.show() + "  F6:" +
           options.charset() + "  F8:" + options.sideView() + "  Tab:Active";
}

std::string App::menuDescription() const {
    switch (menu) {
    case 0:
        return "Compare files starting at current positions";
    case 1:
        return "Display directory";
    case 2:
        return "Edit (in ASCII) file in active window";
    case 3:
        return "Type first letter of command or press \xE2\x97\x84\xE2\x94\x80\xE2\x94\x98 to "
               "select hilited command";
    case 4:
        return "Switch between Colour and Mono monitors";
    case 5:
        return "Repeat Compare or Search";
    case 6:
        return "Change case, direction, data entry and show options";
    case 7:
        return "Move current position forwards";
    case 8:
        return "Move current position backwards";
    case 9:
        return "Set current position";
    case 10:
        return "Quit CABLE, return to DOS";
    case 11:
        return "Search for ASCII string";
    case 12:
        return "View file in active window";
    case 13:
        return "Write changes to active file back to disk";
    default:
        return "";
    }
}

void App::handleKey(const Key& key) {
    if (isScrollLockToggle(key)) {
        options.moveBoth = !options.moveBoth;
        return;
    }
    if (modal) {
        handleModalKey(key);
        return;
    }
    if (edit) {
        handleEditKey(key);
        return;
    }

    if (key.ctrl && key.code == KeyCode::Rune) {
        switch (key.rune) {
        case U'q':
        case U'Q':
            tryQuit();
            break;
        case U's':
        case U'S':
            writeConfirm();
            break;
        default:
            break;
        }
        return;
    }

    if (key.code == KeyCode::Rune) {
        if (key.rune == U' ') {
            menu = (menu + 1) % static_cast<int>(kMenuItems.size());
            clearMessage();
            return;
        }
        if (!key.ctrl) {
            handleCommandChar(key.rune);
            return;
        }
    }

    if (key.isF(1)) {
        openHelp();
    } else if (key.isF(2)) {
        options.respectCase = !options.respectCase;
        setMessage("Case: " + options.caseLabel());
    } else if (key.isF(3)) {
        options.forward = !options.forward;
        setMessage("Direction: " + options.direction());
    } else if (key.isF(4)) {
        options.hexInput = !options.hexInput;
        setMessage("Input: " + options.input());
    } else if (key.isF(5)) {
        options.conceal = !options.conceal;
        setMessage("Unprintables: " + options.show());
    } else if (key.isF(6)) {
        options.ebcdic = !options.ebcdic;
        setMessage("Character set: " + options.charset());
    } else if (key.isF(8)) {
        options.cycleSideView();
        setMessage("Side view: " + options.sideView());
    } else if (key.isF(9)) {
        menu = (menu + static_cast<int>(kMenuItems.size()) - 1) %
               static_cast<int>(kMenuItems.size());
        clearMessage();
    } else if (key.isF(10)) {
        menu = (menu + 1) % static_cast<int>(kMenuItems.size());
        clearMessage();
    } else if (key.code == KeyCode::Tab || key.code == KeyCode::BackTab) {
        switchPane();
    } else if (key.code == KeyCode::Enter) {
        executeMenu(menu);
    } else if (key.code == KeyCode::Esc) {
        if (compareMode) {
            compareMode = false;
            setMessage("Compare display off");
        } else {
            clearMessage();
        }
    } else if (key.code == KeyCode::Left) {
        moveCursor(-1);
    } else if (key.code == KeyCode::Right) {
        moveCursor(1);
    } else if (key.code == KeyCode::Up) {
        moveCursor(-static_cast<long long>(bytesPerRow));
    } else if (key.code == KeyCode::Down) {
        moveCursor(static_cast<long long>(bytesPerRow));
    } else if (key.code == KeyCode::PageUp) {
        moveCursor(-static_cast<long long>(maxInt(rowsPerPane, 1) * maxInt(bytesPerRow, 16)));
    } else if (key.code == KeyCode::PageDown) {
        moveCursor(static_cast<long long>(maxInt(rowsPerPane, 1) * maxInt(bytesPerRow, 16)));
    } else if (key.code == KeyCode::Home) {
        moveHome();
    } else if (key.code == KeyCode::End) {
        moveEnd();
    }
}

void App::switchPane() {
    active = 1 - active;
    panes[active].clampCursor(edit != nullptr);
    panes[active].ensureVisible(rowsPerPane, bytesPerRow);
}

void App::moveCursor(long long delta) {
    bool allowAppend = edit != nullptr;
    int rows = rowsPerPane;
    int bpr = bytesPerRow;
    int act = active;
    auto nudge = [&](Pane& p) {
        if (!p.file) {
            return;
        }
        long long max = static_cast<long long>(p.cursorMax(allowAppend));
        long long next = static_cast<long long>(p.cursor) + delta;
        if (next < 0) {
            next = 0;
        }
        if (next > max) {
            next = max;
        }
        if (next < 0) {
            next = 0;
        }
        p.cursor = static_cast<u64>(next);
        p.ensureVisible(rows, bpr);
    };
    nudge(panes[act]);
    if (bothPanes()) {
        nudge(panes[1 - act]);
    }
}

void App::moveHome() {
    int act = active;
    auto home = [](Pane& p) {
        if (!p.file) {
            return;
        }
        p.cursor = 0;
        p.top = 0;
    };
    home(panes[act]);
    if (bothPanes()) {
        home(panes[1 - act]);
    }
}

void App::moveEnd() {
    bool allowAppend = edit != nullptr;
    int rows = rowsPerPane;
    int bpr = bytesPerRow;
    int act = active;
    auto toEnd = [&](Pane& p) {
        if (!p.file) {
            return;
        }
        p.cursor = p.cursorMax(allowAppend);
        p.ensureVisible(rows, bpr);
    };
    toEnd(panes[act]);
    if (bothPanes()) {
        toEnd(panes[1 - act]);
    }
}

void App::handleCommandChar(char32_t c) {
    switch (c) {
    case U'c':
    case U'C':
        startCompare();
        break;
    case U'd':
    case U'D':
        openDir();
        break;
    case U'e':
    case U'E':
        startEdit();
        break;
    case U'h':
    case U'H':
        openHelp();
        break;
    case U'm':
    case U'M':
        openModes();
        break;
    case U'n':
    case U'N':
        repeatLast();
        break;
    case U'o':
    case U'O':
        openOptions();
        break;
    case U'+':
        openOffsetInput(InputAction::Add);
        break;
    case U'-':
        openOffsetInput(InputAction::Sub);
        break;
    case U'=':
        openOffsetInput(InputAction::Goto);
        break;
    case U'q':
    case U'Q':
        tryQuit();
        break;
    case U's':
    case U'S':
        openSearch();
        break;
    case U'v':
    case U'V':
        openView();
        break;
    case U'w':
    case U'W':
        writeConfirm();
        break;
    default:
        break;
    }
}

void App::executeMenu(int index) {
    int count = static_cast<int>(kMenuItems.size());
    if (index >= count) {
        index = count - 1;
    }
    menu = index;
    switch (index) {
    case 0:
        startCompare();
        break;
    case 1:
        openDir();
        break;
    case 2:
        startEdit();
        break;
    case 3:
        openHelp();
        break;
    case 4:
        openModes();
        break;
    case 5:
        repeatLast();
        break;
    case 6:
        openOptions();
        break;
    case 7:
        openOffsetInput(InputAction::Add);
        break;
    case 8:
        openOffsetInput(InputAction::Sub);
        break;
    case 9:
        openOffsetInput(InputAction::Goto);
        break;
    case 10:
        tryQuit();
        break;
    case 11:
        openSearch();
        break;
    case 12:
        openView();
        break;
    case 13:
        writeConfirm();
        break;
    default:
        break;
    }
}

void App::openHelp() {
    modal = std::make_unique<Modal>();
    modal->kind = ModalKind::Help;
}

void App::openOptions() {
    modal = std::make_unique<Modal>();
    modal->kind = ModalKind::Options;
    modal->index = 0;
}

void App::openModes() {
    modal = std::make_unique<Modal>();
    modal->kind = ModalKind::Modes;
    modal->index = 0;
}

void App::openDir() {
    std::error_code ec;
    fs::path cwd = fs::current_path(ec);
    if (ec) {
        cwd = ".";
    }
    modal = std::make_unique<Modal>();
    modal->kind = ModalKind::Dir;
    modal->dir = std::make_unique<DirBrowser>(cwd, active);
}

void App::openInput(std::string title, std::string prompt, InputAction action,
                    std::string value) {
    modal = std::make_unique<Modal>();
    modal->kind = ModalKind::Input;
    modal->title = std::move(title);
    modal->prompt = std::move(prompt);
    modal->value = std::move(value);
    modal->input = action;
}

void App::openOffsetInput(InputAction action) {
    if (!activePane().file) {
        setMessage("No file to edit");
        return;
    }
    std::string title;
    std::string prompt;
    switch (action) {
    case InputAction::Add:
        title = "+";
        prompt = "+: Enter value to add to current position (if HEX start with H)";
        break;
    case InputAction::Sub:
        title = "-";
        prompt = "-: Enter value to subtract from current position (if HEX start with H)";
        break;
    case InputAction::Goto:
        title = "=";
        prompt = "=: Enter new current position (if HEX start with H)";
        break;
    default:
        break;
    }
    openInput(title, prompt, action, "");
}

void App::openSearch() {
    if (!activePane().file) {
        setMessage("No file to search");
        return;
    }
    std::string prompt;
    if (options.hexInput) {
        prompt = "SEARCH: Enter HEX search string (blanks ignored)";
    } else if (options.ebcdic) {
        prompt = "SEARCH: Enter EBCDIC search string";
    } else {
        prompt = "SEARCH: Enter ASCII search string";
    }
    openInput("SEARCH", prompt, InputAction::Search, "");
}

void App::openView() {
    openInput("VIEW", "VIEW: Enter [drive:][path\\]filespec", InputAction::View, "");
}

std::string App::editPrompt() const {
    if (options.hexInput) {
        return "EDIT: Enter HEX digits (Esc to quit)";
    }
    if (options.ebcdic) {
        return "EDIT: Enter EBCDIC characters (Esc to quit)";
    }
    return "EDIT: Enter ASCII characters (Esc to quit)";
}

void App::startEdit() {
    if (!activePane().file) {
        setMessage("No file to edit");
        return;
    }
    if (options.disasm) {
        setMessage("Edit unavailable in disassembly view");
        return;
    }
    edit = std::make_unique<EditState>();
    message = editPrompt();
    hasMsg = true;
}

void App::handleEditKey(const Key& key) {
    int rows = rowsPerPane;
    int bpr = bytesPerRow;
    if (key.code == KeyCode::Esc) {
        edit.reset();
        clearMessage();
    } else if (key.code == KeyCode::ScrollLock || key.isF(7)) {
        options.moveBoth = !options.moveBoth;
    } else if (key.code == KeyCode::Tab || key.code == KeyCode::BackTab) {
        int other = 1 - active;
        if (!panes[other].file) {
            setMessage("No file to edit");
        } else {
            active = other;
            panes[active].clampCursor(true);
            panes[active].ensureVisible(rows, bpr);
            message = editPrompt();
            hasMsg = true;
        }
    } else if (key.code == KeyCode::Left) {
        moveCursor(-1);
    } else if (key.code == KeyCode::Right) {
        moveCursor(1);
    } else if (key.code == KeyCode::Up) {
        moveCursor(-static_cast<long long>(bpr));
    } else if (key.code == KeyCode::Down) {
        moveCursor(static_cast<long long>(bpr));
    } else if (key.code == KeyCode::PageUp) {
        moveCursor(-static_cast<long long>(maxInt(rows, 1) * maxInt(bpr, 16)));
    } else if (key.code == KeyCode::PageDown) {
        moveCursor(static_cast<long long>(maxInt(rows, 1) * maxInt(bpr, 16)));
    } else if (key.code == KeyCode::Home) {
        moveHome();
    } else if (key.code == KeyCode::End) {
        moveEnd();
    } else if (key.code == KeyCode::Backspace) {
        Pane& p = activePane();
        if (p.cursor > 0) {
            --p.cursor;
        }
        p.ensureVisible(rows, bpr);
    } else if (key.code == KeyCode::Enter) {
        if (!options.hexInput) {
            editByte(0x0D);
        }
    } else if (key.code == KeyCode::Rune && !key.ctrl) {
        if (options.hexInput) {
            int v = -1;
            if (key.rune >= U'0' && key.rune <= U'9') {
                v = static_cast<int>(key.rune - U'0');
            } else if (key.rune >= U'a' && key.rune <= U'f') {
                v = static_cast<int>(key.rune - U'a') + 10;
            } else if (key.rune >= U'A' && key.rune <= U'F') {
                v = static_cast<int>(key.rune - U'A') + 10;
            }
            if (v >= 0) {
                if (edit->has) {
                    u8 hi = edit->nibble;
                    edit->has = false;
                    editByte(static_cast<u8>((hi << 4) | static_cast<u8>(v)));
                } else {
                    edit->nibble = static_cast<u8>(v);
                    edit->has = true;
                }
            }
        } else {
            std::vector<u8> bytes;
            std::string error;
            if (!encodeText(utf8Encode(key.rune), options.ebcdic, bytes, error)) {
                setMessage(error);
            } else if (!bytes.empty()) {
                editByte(bytes[0]);
            }
        }
    }
}

void App::editByte(u8 value) {
    int idx = active;
    int rows = rowsPerPane;
    int bpr = bytesPerRow;
    if (!panes[idx].file) {
        setMessage("No file to edit");
        return;
    }
    Pane& p = panes[idx];
    std::string error;
    if (!p.file->writeOffset(p.cursor, value, error)) {
        setMessage("Error writing file: " + error);
        return;
    }
    ++p.cursor;
    p.ensureVisible(rows, bpr);
}

void App::startCompare() {
    if (!panes[0].file || !panes[1].file) {
        setMessage("No file to compare");
        return;
    }
    compareMode = true;
    last = std::make_unique<LastAction>();
    last->kind = LastActionKind::Compare;
    doCompare(false);
}

void App::doCompare(bool advance) {
    if (!panes[0].file || !panes[1].file) {
        setMessage("No file to compare");
        return;
    }
    u64 cursor = activePane().cursor;
    bool forward = options.forward;
    u64 start = 0;
    if (!advance) {
        start = cursor;
    } else if (forward) {
        start = cursor + 1;
    } else if (cursor == 0) {
        setMessage("No difference found before end of file");
        return;
    } else {
        start = cursor - 1;
    }
    int act = active;
    std::optional<u64> offset;
    if (act == 0) {
        offset = findDifference(*panes[0].file, *panes[1].file, start, forward);
    } else {
        offset = findDifference(*panes[1].file, *panes[0].file, start, forward);
    }
    if (!offset) {
        setMessage("No difference found before end of file");
        return;
    }
    compareMode = true;
    u64 bpr = static_cast<u64>(maxInt(bytesPerRow, 16));
    for (auto& pane : panes) {
        pane.cursor = *offset;
        if (pane.length() < pane.cursor) {
            pane.cursor = pane.length();
        }
        if (pane.file) {
            pane.top = (*offset / bpr) * bpr;
            pane.ensureVisible(rowsPerPane, static_cast<int>(bpr));
        }
    }
    clearMessage();
}

void App::repeatLast() {
    if (!last) {
        setMessage("No compare/search to repeat");
        return;
    }
    if (last->kind == LastActionKind::Search) {
        performSearch(last->needle, true);
    } else {
        doCompare(true);
    }
}

void App::performSearch(const std::vector<u8>& needle, bool advance) {
    if (needle.empty()) {
        setMessage("Search requires a string");
        return;
    }
    bool forward = options.forward;
    u64 cursor = activePane().cursor;
    u64 start = 0;
    if (!advance) {
        start = cursor;
    } else if (forward) {
        start = cursor + 1;
    } else if (cursor == 0) {
        setMessage("No match found before end of file");
        return;
    } else {
        start = cursor - 1;
    }
    PagedFile* file = activePane().file.get();
    if (file == nullptr) {
        setMessage("No file to search");
        return;
    }
    auto offset = findBytes(*file, needle, start, forward, options.respectCase, options.ebcdic);
    if (!offset) {
        setMessage("No match found before end of file");
        return;
    }
    Pane& p = activePane();
    p.cursor = *offset;
    p.ensureVisible(rowsPerPane, bytesPerRow);
    clearMessage();
}

void App::writeConfirm() {
    Pane& p = activePane();
    if (!p.file) {
        setMessage("No file to edit");
        return;
    }
    if (!p.file->dirty()) {
        setMessage("No changes to write");
        return;
    }
    if (p.file->readOnly()) {
        setMessage("File is read-only - can't write");
        return;
    }
    modal = std::make_unique<Modal>();
    modal->kind = ModalKind::Confirm;
    modal->title = "WRITE";
    modal->text = "Do you want to write changes to " + p.file->path().string() + " (Y/N)?";
    modal->confirm = ConfirmAction::Write;
}

void App::doWrite() {
    Pane& p = activePane();
    if (!p.file) {
        setMessage("No file to edit");
        return;
    }
    std::string error;
    u64 length = p.file->save(error);
    if (!error.empty()) {
        setMessage(error);
        return;
    }
    setMessage(strprintf("%llu bytes written", static_cast<unsigned long long>(length)));
}

void App::tryQuit() {
    if (anyDirty()) {
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Confirm;
        modal->title = "QUIT";
        modal->text = "Changes lost.  Quit without writing changes? (Y/N)";
        modal->confirm = ConfirmAction::Quit;
    } else {
        quit = true;
    }
}

void App::loadInto(fs::path path, int pane) {
    if (panes[pane].dirty()) {
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Confirm;
        modal->title = "REPLACE";
        modal->text = "Changes to " + panes[pane].name() + " not written.  Replace it? (Y/N)";
        modal->confirm = ConfirmAction::Load;
        modal->path = path.string();
        modal->pane = pane;
        return;
    }
    loadIntoForce(std::move(path), pane);
}

void App::loadIntoForce(fs::path path, int pane) {
    std::string error;
    auto file = PagedFile::open(path, error);
    if (!file) {
        setMessage("Error opening file: " + error);
        return;
    }
    panes[pane].file = std::move(file);
    panes[pane].cursor = 0;
    panes[pane].top = 0;
    compareMode = false;
    clearMessage();
}

static std::string trimLastRune(const std::string& text) {
    auto runes = utf8Decode(text);
    if (runes.empty()) {
        return text;
    }
    runes.pop_back();
    std::string out;
    for (char32_t c : runes) {
        out += utf8Encode(c);
    }
    return out;
}

static std::string trimSpace(std::string_view text) {
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

void App::submitInput(InputAction action, const std::string& value) {
    switch (action) {
    case InputAction::Add:
    case InputAction::Sub:
    case InputAction::Goto: {
        u64 amount = 0;
        std::string error;
        if (!parseOffset(value, amount, error)) {
            setMessage(error);
            return;
        }
        Pane& p = activePane();
        if (!p.file) {
            setMessage("No file to edit");
            return;
        }
        u64 length = p.length();
        u64 max = 0;
        if (edit) {
            max = length;
        } else if (length > 0) {
            max = length - 1;
        }
        switch (action) {
        case InputAction::Add:
            p.cursor += amount;
            if (p.cursor > max) {
                p.cursor = max;
            }
            break;
        case InputAction::Sub:
            if (p.cursor > amount) {
                p.cursor -= amount;
            } else {
                p.cursor = 0;
            }
            break;
        case InputAction::Goto:
            p.cursor = amount;
            if (p.cursor > max) {
                p.cursor = max;
            }
            break;
        default:
            break;
        }
        p.ensureVisible(rowsPerPane, bytesPerRow);
        clearMessage();
        break;
    }
    case InputAction::Search: {
        std::string trimmed = value;
        if (trimmed.find_first_not_of(" \t\r\n") == std::string::npos) {
            setMessage("Search requires a string");
            return;
        }
        std::vector<u8> needle;
        std::string error;
        bool ok = options.hexInput ? parseHex(value, needle, error)
                                   : encodeText(value, options.ebcdic, needle, error);
        if (!ok) {
            setMessage(error);
            return;
        }
        if (needle.empty()) {
            setMessage("Search requires a string");
            return;
        }
        last = std::make_unique<LastAction>();
        last->kind = LastActionKind::Search;
        last->needle = needle;
        performSearch(needle, false);
        break;
    }
    case InputAction::View: {
        if (value.find_first_not_of(" \t\r\n") == std::string::npos) {
            setMessage("View requires a filespec");
            return;
        }
        fs::path path = trimSpace(value);
        if (!path.is_absolute()) {
            std::error_code ec;
            fs::path cwd = fs::current_path(ec);
            if (!ec) {
                path = cwd / path;
            }
        }
        std::error_code ec;
        if (fs::is_directory(path, ec)) {
            setMessage("Error opening file: is a directory");
        } else {
            loadInto(path, active);
        }
        break;
    }
    }
}

void App::handleModalKey(const Key& key) {
    if (!modal) {
        return;
    }
    Modal m = std::move(*modal);
    modal.reset();

    switch (m.kind) {
    case ModalKind::Confirm:
        if ((key.code == KeyCode::Rune && (key.rune == U'y' || key.rune == U'Y')) ||
            key.code == KeyCode::Enter) {
            switch (m.confirm) {
            case ConfirmAction::Quit:
                quit = true;
                break;
            case ConfirmAction::Write:
                doWrite();
                break;
            case ConfirmAction::Load:
                loadIntoForce(m.path, m.pane);
                break;
            }
        } else if ((key.code == KeyCode::Rune && (key.rune == U'n' || key.rune == U'N')) ||
                   key.code == KeyCode::Esc) {
            // leave the modal closed
        } else {
            modal = std::make_unique<Modal>(std::move(m));
        }
        break;
    case ModalKind::Input:
        if (key.code == KeyCode::Esc) {
            // closed
        } else if (key.code == KeyCode::Enter) {
            submitInput(m.input, m.value);
        } else if (key.code == KeyCode::Backspace) {
            m.value = trimLastRune(m.value);
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Rune && key.ctrl &&
                   (key.rune == U'u' || key.rune == U'U')) {
            m.value.clear();
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Rune && !key.ctrl) {
            m.value += utf8Encode(key.rune);
            modal = std::make_unique<Modal>(std::move(m));
        } else {
            modal = std::make_unique<Modal>(std::move(m));
        }
        break;
    case ModalKind::Options: {
        const int n = 11;
        if (key.code == KeyCode::Esc) {
            // closed
        } else if (key.code == KeyCode::Up) {
            m.index = (m.index + n - 1) % n;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Down || key.code == KeyCode::Tab) {
            m.index = (m.index + 1) % n;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Enter ||
                   (key.code == KeyCode::Rune && key.rune == U' ')) {
            activateOption(m.index);
        } else if (key.code == KeyCode::Rune) {
            optionLetter(key.rune);
        } else {
            modal = std::make_unique<Modal>(std::move(m));
        }
        break;
    }
    case ModalKind::Modes: {
        const int n = 5;
        if (key.code == KeyCode::Esc) {
            // closed
        } else if (key.code == KeyCode::Up) {
            m.index = (m.index + n - 1) % n;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Down || key.code == KeyCode::Tab) {
            m.index = (m.index + 1) % n;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Enter ||
                   (key.code == KeyCode::Rune && key.rune == U' ')) {
            activateMode(m.index);
        } else if (key.code == KeyCode::Rune) {
            bool keep = false;
            switch (key.rune) {
            case U'a':
            case U'A':
                activateMode(0);
                break;
            case U'c':
            case U'C':
                activateMode(1);
                break;
            case U'h':
            case U'H':
                activateMode(2);
                break;
            case U'm':
            case U'M':
                activateMode(3);
                break;
            case U'q':
            case U'Q':
                activateMode(4);
                break;
            default:
                keep = true;
                break;
            }
            if (keep) {
                modal = std::make_unique<Modal>(std::move(m));
            }
        } else {
            modal = std::make_unique<Modal>(std::move(m));
        }
        break;
    }
    case ModalKind::Attributes: {
        const int n = 3;
        if (key.code == KeyCode::Esc) {
            // closed
        } else if (key.code == KeyCode::Up) {
            m.index = (m.index + n - 1) % n;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Down || key.code == KeyCode::Tab) {
            m.index = (m.index + 1) % n;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Enter ||
                   (key.code == KeyCode::Rune && key.rune == U' ')) {
            if (m.index == 0) {
                options.theme = Theme::Classic;
            } else if (m.index == 1) {
                options.theme = Theme::Amber;
            } else {
                options.theme = Theme::Mono;
            }
        } else {
            modal = std::make_unique<Modal>(std::move(m));
        }
        break;
    }
    case ModalKind::Help: {
        int max = static_cast<int>(kHelpLines.size()) - 1;
        if (key.code == KeyCode::Esc ||
            (key.code == KeyCode::Rune && (key.rune == U'q' || key.rune == U'Q')) ||
            key.isF(1)) {
            // closed
        } else if (key.code == KeyCode::Up) {
            if (m.scroll > 0) {
                --m.scroll;
            }
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Down) {
            ++m.scroll;
            if (m.scroll > max) {
                m.scroll = max;
            }
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::PageUp) {
            m.scroll -= 20;
            if (m.scroll < 0) {
                m.scroll = 0;
            }
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::PageDown) {
            m.scroll += 20;
            if (m.scroll > max) {
                m.scroll = max;
            }
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::Home) {
            m.scroll = 0;
            modal = std::make_unique<Modal>(std::move(m));
        } else if (key.code == KeyCode::End) {
            m.scroll = max;
            modal = std::make_unique<Modal>(std::move(m));
        } else {
            modal = std::make_unique<Modal>(std::move(m));
        }
        break;
    }
    case ModalKind::Dir:
        modal = std::make_unique<Modal>(std::move(m));
        handleDirKey(*modal, key);
        break;
    }
}

void App::handleDirKey(Modal& m, const Key& key) {
    DirBrowser* b = m.dir.get();
    if (b == nullptr) {
        return;
    }
    const int page = 15;
    if (b->pathInput()) {
        if (key.code == KeyCode::Esc) {
            b->setPathInput(std::nullopt);
        } else if (key.code == KeyCode::Enter) {
            b->applyPathInput();
        } else if (key.code == KeyCode::Backspace) {
            b->setPathInput(trimLastRune(*b->pathInput()));
        } else if (key.code == KeyCode::Rune && !key.ctrl) {
            b->setPathInput(*b->pathInput() + utf8Encode(key.rune));
        }
        return;
    }

    if (b->sortPrompt()) {
        if (key.code == KeyCode::Esc) {
            b->setSortPrompt(false);
        } else if (key.code == KeyCode::Rune) {
            switch (key.rune) {
            case U'n':
            case U'N':
                b->setSort(SortKey::Name);
                break;
            case U'e':
            case U'E':
                b->setSort(SortKey::Ext);
                break;
            case U'd':
            case U'D':
                b->setSort(SortKey::Date);
                break;
            case U's':
            case U'S':
                b->setSort(SortKey::Size);
                break;
            default:
                break;
            }
        }
        return;
    }

    if (key.code == KeyCode::Esc) {
        modal.reset();
    } else if (key.code == KeyCode::Up) {
        b->moveBy(-1);
        b->ensureVisible(page);
    } else if (key.code == KeyCode::Down) {
        b->moveBy(1);
        b->ensureVisible(page);
    } else if (key.code == KeyCode::PageUp) {
        b->moveBy(-page);
        b->ensureVisible(page);
    } else if (key.code == KeyCode::PageDown) {
        b->moveBy(page);
        b->ensureVisible(page);
    } else if (key.code == KeyCode::Home) {
        b->home();
        b->ensureVisible(page);
    } else if (key.code == KeyCode::End) {
        b->end();
        b->ensureVisible(page);
    } else if (key.code == KeyCode::Backspace) {
        b->parent();
    } else if (key.code == KeyCode::Tab) {
        b->toggleTarget();
    } else if (key.code == KeyCode::Rune && (key.rune == U's' || key.rune == U'S')) {
        b->setSortPrompt(true);
    } else if (key.code == KeyCode::Rune && key.rune == U'/') {
        b->beginPathInput();
    } else if (key.code == KeyCode::Enter) {
        int target = b->targetPane();
        bool view = key.ctrl;
        if (view) {
            auto path = b->currentPath();
            if (path) {
                loadInto(*path, 1 - target);
            }
        } else {
            auto path = b->activate();
            if (path) {
                loadInto(*path, target);
            }
        }
    }
}

void App::activateOption(int index) {
    switch (index) {
    case 0:
        options.ebcdic = false;
        break;
    case 1:
        options.forward = false;
        break;
    case 2:
        options.conceal = true;
        break;
    case 3:
        options.ebcdic = true;
        break;
    case 4:
        options.forward = true;
        break;
    case 5:
        modal.reset();
        openHelp();
        return;
    case 6:
        options.respectCase = false;
        break;
    case 7:
        return;
    case 8:
        options.respectCase = true;
        break;
    case 9:
        options.conceal = false;
        break;
    case 10:
        options.hexInput = !options.hexInput;
        break;
    default:
        break;
    }
    if (!modal) {
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Options;
        modal->index = index;
    }
}

void App::optionLetter(char32_t c) {
    switch (c) {
    case U'a':
    case U'A':
        options.ebcdic = false;
        break;
    case U'e':
    case U'E':
        options.ebcdic = true;
        break;
    case U'b':
    case U'B':
        options.forward = false;
        break;
    case U'f':
    case U'F':
        options.forward = true;
        break;
    case U'c':
    case U'C':
        options.conceal = true;
        break;
    case U's':
    case U'S':
        options.conceal = false;
        break;
    case U'i':
    case U'I':
        options.respectCase = false;
        break;
    case U'r':
    case U'R':
        options.respectCase = true;
        break;
    case U'x':
    case U'X':
        options.hexInput = !options.hexInput;
        break;
    case U'h':
    case U'H':
        modal.reset();
        openHelp();
        return;
    case U'q':
    case U'Q':
        return;
    default:
        break;
    }
    if (!modal) {
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Options;
        modal->index = 0;
    }
}

void App::activateMode(int index) {
    switch (index) {
    case 0:
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Attributes;
        modal->index = 0;
        break;
    case 1:
        options.theme = Theme::Classic;
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Modes;
        modal->index = 1;
        break;
    case 2:
        modal.reset();
        openHelp();
        break;
    case 3:
        options.theme = Theme::Mono;
        modal = std::make_unique<Modal>();
        modal->kind = ModalKind::Modes;
        modal->index = 3;
        break;
    case 4:
        modal.reset();
        break;
    default:
        break;
    }
}
