// Terminal rendering.

#include "cable.hpp"
#include "help_text.hpp"

#include <utility>

namespace {
constexpr int kFooterRows = 4;
constexpr int kPreviewLen = 64;
constexpr int kPreviewBefore = 16;
constexpr int kOriginalCols = 80;
constexpr int kOriginalRows = 25;
constexpr int kPaneRows = 8;
constexpr int kCursorRow = 4;
constexpr int kCursorSlot = 5;
constexpr int kViewAhead = kCursorRow * 16 + kCursorSlot;
constexpr int kDataCol = 15;
constexpr int kAsciiCol = 64;

std::optional<u8> byteAt(App& app, int index, u64 offset) {
    if (index < 0 || index >= 2) {
        return std::nullopt;
    }
    Pane& pane = app.panes[index];
    if (!pane.file) {
        return std::nullopt;
    }
    return pane.file->readOffset(offset);
}

std::string hexValue(u64 offset) {
    return strprintf("Hex %08X", static_cast<unsigned int>(offset));
}

std::string decValue(u64 offset) {
    std::string digits = strprintf("%llu", static_cast<unsigned long long>(offset));
    if (digits.size() > 8) {
        digits = digits.substr(digits.size() - 8);
    }
    return strprintf("Dec %8s", digits.c_str());
}

std::string previewText(App& app, int index) {
    u64 cursor = app.panes[index].cursor;
    long long start = static_cast<long long>(cursor) - kPreviewBefore;
    std::string out;
    out.reserve(kPreviewLen);
    for (int i = 0; i < kPreviewLen; ++i) {
        long long off = start + i;
        if (off < 0) {
            out.push_back(' ');
            continue;
        }
        if (auto v = byteAt(app, index, static_cast<u64>(off))) {
            out += utf8Encode(displayByte(*v, app.options.ebcdic, app.options.conceal));
        } else {
            out.push_back(' ');
        }
    }
    return out;
}

std::optional<std::pair<std::string, std::string>> crosshairChars(int index, int row) {
    if (index == 0) {
        if (row == kCursorRow) {
            return std::make_pair("\xE2\x95\x94", "\xE2\x95\x97");
        }
        if (row > kCursorRow) {
            return std::make_pair("\xE2\x95\x91", "\xE2\x95\x91");
        }
        return std::nullopt;
    }
    if (row == kCursorRow) {
        return std::make_pair("\xE2\x95\x9A", "\xE2\x95\x9D");
    }
    if (row < kCursorRow) {
        return std::make_pair("\xE2\x95\x91", "\xE2\x95\x91");
    }
    return std::nullopt;
}

void drawOriginalTitle(Screen& s, App& app, int index, int x, int y) {
    Theme theme = app.options.theme;
    bool active = app.active == index;
    Style style = active ? barStyle(theme) : baseStyle(theme);
    Pane& pane = app.panes[index];
    std::string name;
    if (pane.file) {
        name = pane.file->path().filename().string();
    } else {
        name = pane.name();
    }
    u64 length = pane.length();
    bool readOnly = pane.file && pane.file->readOnly();
    std::string attrs = readOnly ? "R" : "A";
    name = truncateRunes(name, 56);
    if (pane.dirty()) {
        name = truncateRunes("*" + name, 56);
    }
    s.put(x, y, padRight(name, 56), style);
    s.put(x + 57, y,
          strprintf("%8llu %08llX", static_cast<unsigned long long>(length),
                    static_cast<unsigned long long>(length)),
          style);
    s.put(x + 75, y, strprintf("%4s ", attrs.c_str()), style);
}

void drawOriginalStrip(Screen& s, App& app, int x, int y) {
    Theme theme = app.options.theme;
    Style style = baseStyle(theme);
    struct Entry {
        std::string label;
        std::string value;
        int width;
    };
    Entry entries[] = {
        {"F2:", app.options.caseLabel(), 7},
        {"F3:", app.options.direction(), 8},
        {"F4:", app.options.input(), 9},
        {"F5:", app.options.show(), 7},
        {"F6:", app.options.charset(), 6},
        {"SL:", app.options.scrollLock(), 6},
    };
    for (int i = 0; i < 6; ++i) {
        s.put(x, y + 2 + i,
              entries[i].label + padRight(entries[i].value, static_cast<std::size_t>(entries[i].width)),
              style);
    }
    const char* banner[] = {
        "CABLE V1.7X", "IBM Internal", "Use Only", "", "Dave Mitchell", "ZENO @ WINVMB",
    };
    for (int i = 0; i < 6; ++i) {
        s.put(x, y + 14 + i, banner[i], style);
    }
}

void drawOriginalBody(Screen& s, App& app, int index, int x, int y) {
    Theme theme = app.options.theme;
    bool active = app.active == index;
    bool compare = app.compareMode;
    bool ebcdic = app.options.ebcdic;
    bool conceal = app.options.conceal;
    u64 cursor = app.panes[index].cursor;
    long long start = static_cast<long long>(cursor) - kViewAhead;
    Style normal = baseStyle(theme);
    Style dim{themeDim(theme), themeBG(theme), false};
    Style diff{themeDiffFG(theme), themeDiffBG(theme), false};

    if (!app.panes[index].file) {
        s.put(x, y, "--- empty ---", dim);
        return;
    }

    std::vector<std::vector<Cell>> disasmCells;
    if (app.options.disasm) {
        disasmCells = disasmWindow(app, index, kPaneRows, 16);
    }

    for (int row = 0; row < kPaneRows; ++row) {
        long long rowStart = start + static_cast<long long>(row) * 16;
        int lineY = y + row;
        for (int i = 0; i < 16; ++i) {
            long long off = rowStart + i;
            bool blank = off < 0 || static_cast<u64>(off) >= app.panes[index].length();
            std::optional<u8> b;
            if (!blank) {
                b = byteAt(app, index, static_cast<u64>(off));
            }
            std::optional<u8> other;
            if (compare && !blank) {
                other = byteAt(app, 1 - index, static_cast<u64>(off));
            }
            int bx = x + kDataCol + i * 3;
            if (b) {
                Style style = normal;
                if (compare && (!other || *b != *other)) {
                    style = diff;
                }
                s.put(bx, lineY, strprintf("%02X", *b), style);
                if (!app.options.disasm) {
                    s.set(x + kAsciiCol + i, lineY, displayByte(*b, ebcdic, conceal), style);
                }
            } else {
                s.put(bx, lineY, "**", normal);
                if (!app.options.disasm) {
                    s.set(x + kAsciiCol + i, lineY, U' ', normal);
                }
            }
        }

        if (auto cross = crosshairChars(index, row)) {
            s.put(x + kDataCol + 14, lineY, cross->first, normal);
            s.put(x + kDataCol + 17, lineY, cross->second, normal);
        }

        if (active && row == kCursorRow) {
            int cx = x + kDataCol + kCursorSlot * 3;
            s.restyle(Rect{cx, lineY, 2, 1}, barStyle(theme));
        }
    }

    if (!disasmCells.empty()) {
        for (std::size_t row = 0; row < disasmCells.size() && row < static_cast<std::size_t>(kPaneRows);
             ++row) {
            for (std::size_t i = 0; i < disasmCells[row].size(); ++i) {
                const Cell& cell = disasmCells[row][i];
                s.set(x + kAsciiCol + static_cast<int>(i), y + static_cast<int>(row), cell.ch,
                      cell.style);
            }
        }
    }
}

void drawOriginalBand(Screen& s, App& app, int x, int y) {
    Theme theme = app.options.theme;
    Style normal = baseStyle(theme);
    Style bar = barStyle(theme);
    std::string hex0 = hexValue(app.panes[0].cursor);
    std::string dec0 = decValue(app.panes[0].cursor);
    std::string hex1 = hexValue(app.panes[1].cursor);
    std::string dec1 = decValue(app.panes[1].cursor);
    const char* TL = "\xE2\x95\x94"; // ╔
    const char* TR = "\xE2\x95\x97"; // ╗
    const char* BL = "\xE2\x95\x9A"; // ╚
    const char* BR = "\xE2\x95\x9D"; // ╝
    const char* V = "\xE2\x95\x91";  // ║
    const char* H = "\xE2\x95\x90";  // ═
    const char* ML = "\xE2\x95\xA0"; // ╠
    const char* BT = "\xE2\x95\xA9"; // ╩
    const char* TT = "\xE2\x95\xA6"; // ╦
    auto rep = [](const char* s, int n) {
        std::string out;
        for (int i = 0; i < n; ++i) {
            out += s;
        }
        return out;
    };
    std::string lines[6] = {
        std::string(TL) + rep(H, 12) + TR,
        std::string(V) + hex0 + ML + rep(H, 15) + BT + H + TT + BR,
        std::string(BL) + dec0 + V,
        std::string(TL) + dec1 + V,
        std::string(V) + hex1 + ML + rep(H, 15) + TT + H + BT + TR,
        std::string(BL) + rep(H, 12) + BR,
    };
    for (int i = 0; i < 6; ++i) {
        s.put(x, y + i, lines[i], normal);
    }

    int hexRow = 1;
    int decRow = 2;
    if (app.active == 1) {
        hexRow = 4;
        decRow = 3;
    }
    int rows[2] = {hexRow, decRow};
    for (int row : rows) {
        s.restyle(Rect{x + 5, y + row, 8, 1}, bar);
    }

    for (int index = 0; index < 2; ++index) {
        int row = index == 1 ? 3 : 2;
        bool active = app.active == index;
        Style style = normal;
        if (active) {
            style = bar;
            s.restyle(Rect{x + kDataCol, y + row, kPreviewLen, 1}, bar);
        }
        s.put(x + kDataCol, y + row, previewText(app, index), style);
    }
}

void drawMenuLine(Screen& s, App& app, int x, int y);

void drawOriginalFooter(Screen& s, App& app, int x, int y) {
    Theme theme = app.options.theme;
    drawMenuLine(s, app, x, y);
    std::string text;
    Style style = baseStyle(theme);
    if (app.hasMsg) {
        text = app.message;
        style = Style{themeHeaderFG(theme), themeBG(theme), true};
    } else {
        text = app.menuDescription();
    }
    s.put(x, y + 1, text, style);
}

void drawOriginal(Screen& s, App& app) {
    app.syncGeometry(kPaneRows, 16);
    drawOriginalTitle(s, app, 0, 0, 0);
    drawOriginalTitle(s, app, 1, 0, 21);
    drawOriginalBody(s, app, 0, 0, 1);
    drawOriginalBody(s, app, 1, 0, 13);
    drawOriginalStrip(s, app, 0, 0);
    drawOriginalBand(s, app, 0, 8);
    drawOriginalFooter(s, app, 0, 22);
}

int adaptiveBytesPerRowLocal(int width) {
    int groups = (width - 9) / 50;
    if (width < 9) {
        groups = 0;
    }
    if (groups < 1) {
        groups = 1;
    }
    if (groups > 8) {
        groups = 8;
    }
    return groups * 16;
}

static std::pair<int, int> adaptiveOffsetWidths(App& app) {
    u64 max = std::max(app.panes[0].length(), app.panes[1].length());
    int hex = static_cast<int>(strprintf("%llX", static_cast<unsigned long long>(max)).size());
    if (hex < 8) {
        hex = 8;
    }
    int dec = static_cast<int>(strprintf("%llu", static_cast<unsigned long long>(max)).size());
    if (dec < 1) {
        dec = 1;
    }
    return {hex, dec};
}

void drawAdaptiveMiddle(Screen& s, App& app, const Rect& rect) {
    if (rect.w == 0 || rect.h == 0) {
        return;
    }
    Theme theme = app.options.theme;
    Style normal = baseStyle(theme);
    Style cursorStyle{themeCursorFG(theme), themeCursorBG(theme), true};

    auto widths = adaptiveOffsetWidths(app);
    int hexWidth = widths.first;
    int decWidth = widths.second;
    int dcol = hexWidth + 1;
    int center = adaptiveHexCenter(app.bytesPerRow, dcol);
    int previewCol = center - kPreviewBefore;
    if (previewCol < 0) {
        previewCol = 0;
    }

    for (int index = 0; index < 2; ++index) {
        if (index >= rect.h) {
            break;
        }
        int y = rect.y + index;
        bool active = app.active == index;
        Style style = normal;
        if (active) {
            style = barStyle(theme);
            s.restyle(Rect{rect.x, y, rect.w, 1}, style);
        }

        u64 cursor = app.panes[index].cursor;
        std::string head =
            strprintf("Hex %0*llX  Dec %*llu", hexWidth,
                      static_cast<unsigned long long>(cursor), decWidth,
                      static_cast<unsigned long long>(cursor));
        int headLen = static_cast<int>(runeLen(head));
        int start = previewCol;
        if (start < headLen + 2) {
            start = headLen + 2;
        }
        s.put(rect.x, y, head, style);
        if (start < rect.w) {
            if (start > headLen) {
                s.put(rect.x + headLen, y, std::string(static_cast<std::size_t>(start - headLen), ' '),
                      style);
            }
            int room = rect.w - start;
            if (room > kPreviewLen) {
                room = kPreviewLen;
            }
            auto preview = utf8Decode(previewText(app, index));
            for (int i = 0; i < room && i < static_cast<int>(preview.size()); ++i) {
                Style cellStyle = style;
                if (i == kPreviewBefore) {
                    cellStyle = cursorStyle;
                }
                s.set(rect.x + start + i, y, preview[static_cast<std::size_t>(i)], cellStyle);
            }
        }
    }
}

void hexLine(Screen& s, App& app, int index, u64 offset, int x, int y) {
    Theme theme = app.options.theme;
    bool active = app.active == index;
    u64 cursor = app.panes[index].cursor;
    bool compare = app.compareMode;
    int bpr = app.bytesPerRow;
    if (bpr < 16) {
        bpr = 16;
    }

    Style normal = baseStyle(theme);
    Style dim{themeDim(theme), themeBG(theme), false};
    Style cursorStyle{themeCursorFG(theme), themeCursorBG(theme), true};
    Style diffStyle{themeDiffFG(theme), themeDiffBG(theme), false};
    Style offsetStyle{themeAccent(theme), themeBG(theme), false};

    int cx = x;
    cx = s.put(cx, y, strprintf("%08llX ", static_cast<unsigned long long>(offset)), offsetStyle);

    std::vector<u8> bytes(static_cast<std::size_t>(bpr), 0);
    std::vector<bool> has(static_cast<std::size_t>(bpr), false);
    std::vector<u8> other(static_cast<std::size_t>(bpr), 0);
    std::vector<bool> otherHas(static_cast<std::size_t>(bpr), false);
    for (int i = 0; i < bpr; ++i) {
        if (auto b = byteAt(app, index, offset + static_cast<u64>(i))) {
            bytes[static_cast<std::size_t>(i)] = *b;
            has[static_cast<std::size_t>(i)] = true;
        }
        if (compare) {
            if (auto b = byteAt(app, 1 - index, offset + static_cast<u64>(i))) {
                other[static_cast<std::size_t>(i)] = *b;
                otherHas[static_cast<std::size_t>(i)] = true;
            }
        }
    }

    std::vector<Style> styles(static_cast<std::size_t>(bpr), normal);
    for (int i = 0; i < bpr; ++i) {
        u64 off = offset + static_cast<u64>(i);
        bool isCursor = active && off == cursor;
        bool differs = compare && (has[static_cast<std::size_t>(i)] !=
                                       otherHas[static_cast<std::size_t>(i)] ||
                                   (has[static_cast<std::size_t>(i)] &&
                                    bytes[static_cast<std::size_t>(i)] !=
                                        other[static_cast<std::size_t>(i)]));
        if (isCursor) {
            styles[static_cast<std::size_t>(i)] = cursorStyle;
        } else if (differs) {
            styles[static_cast<std::size_t>(i)] = diffStyle;
        }
    }

    for (int i = 0; i < bpr; ++i) {
        if (i > 0 && i % 16 == 0) {
            s.set(cx, y, U' ', normal);
            ++cx;
        }
        if (has[static_cast<std::size_t>(i)]) {
            s.put(cx, y, strprintf("%02X", bytes[static_cast<std::size_t>(i)]),
                  styles[static_cast<std::size_t>(i)]);
            cx += 2;
        } else {
            s.put(cx, y, "**", dim);
            cx += 2;
        }
    }

    cx = s.put(cx, y, "  ", normal);
    if (!app.options.disasm) {
        for (int i = 0; i < bpr; ++i) {
            if (i > 0 && i % 16 == 0) {
                s.set(cx, y, U' ', normal);
                ++cx;
            }
            if (has[static_cast<std::size_t>(i)]) {
                s.set(cx, y,
                      displayByte(bytes[static_cast<std::size_t>(i)], app.options.ebcdic,
                                  app.options.conceal),
                      styles[static_cast<std::size_t>(i)]);
            } else {
                s.set(cx, y, U' ', dim);
            }
            ++cx;
        }
    }
    (void)cursor;
}

void drawPane(Screen& s, App& app, int index, const Rect& rect) {
    if (rect.h == 0 || rect.w == 0) {
        return;
    }
    Theme theme = app.options.theme;
    bool active = app.active == index;
    Pane& pane = app.panes[index];
    u64 cursor = pane.cursor;
    u64 top = pane.top;
    std::string name = pane.name();
    u64 length = pane.length();
    bool dirty = pane.dirty();
    bool readOnly = pane.file && pane.file->readOnly();

    bool headerAtBottom = index == 1;
    int headerY = rect.y;
    if (headerAtBottom) {
        headerY = rect.y + satSub(rect.h, 1);
    }
    Rect header{rect.x, headerY, rect.w, 1};

    std::string titleFG = active ? themeBG(theme) : themeDim(theme);
    std::string titleBG = active ? themeHeaderFG(theme) : themeBG(theme);
    Style titleStyle{titleFG, titleBG, active};
    s.restyle(header, titleStyle);

    int rightW = 38;
    if (rightW > rect.w) {
        rightW = rect.w;
    }
    std::string titleText =
        name + strprintf("  [%llu bytes]", static_cast<unsigned long long>(length));
    if (dirty) {
        titleText += " *";
    }
    if (readOnly) {
        titleText += " (read-only)";
    }
    s.putLimit(header.x, header.y, titleText, titleStyle, satSub(rect.w, rightW));

    std::string flagChar = active ? "A" : " ";
    std::string cmp = app.compareMode ? " CMP" : "";
    std::string flags =
        strprintf("Hex %08llX  Dec %llu  %s%s", static_cast<unsigned long long>(cursor),
                  static_cast<unsigned long long>(cursor), flagChar.c_str(), cmp.c_str());
    int flagX = header.x + satSub(rect.w, rightW);
    s.putLimit(flagX, header.y, flags, titleStyle, rightW);

    Rect body{rect.x, rect.y + 1, rect.w, satSub(rect.h, 1)};
    if (headerAtBottom) {
        body = Rect{rect.x, rect.y, rect.w, satSub(rect.h, 1)};
    }
    if (!pane.file) {
        std::string text = "--- empty ---";
        int x = body.x + satSub(body.w, static_cast<int>(runeLen(text))) / 2;
        s.put(x, body.y + body.h / 2, text, Style{themeDim(theme), themeBG(theme), false});
        return;
    }
    int rows = body.h;
    if (rows == 0) {
        return;
    }

    int bpr = app.bytesPerRow;
    if (bpr < 16) {
        bpr = 16;
    }
    u64 bpr64 = static_cast<u64>(bpr);
    for (int row = 0; row < rows; ++row) {
        u64 offset = top + static_cast<u64>(row) * bpr64;
        hexLine(s, app, index, offset, body.x, body.y + row);
    }

    if (app.options.disasm) {
        int groups = bpr / 16;
        if (groups < 1) {
            groups = 1;
        }
        int prefix = 9 + (33 * groups - 1) + 2;
        if (prefix < body.w) {
            int width = body.w - prefix;
            auto window = disasmWindow(app, index, rows, width);
            for (std::size_t row = 0; row < window.size(); ++row) {
                for (std::size_t i = 0; i < window[row].size(); ++i) {
                    const Cell& cell = window[row][i];
                    s.set(body.x + prefix + static_cast<int>(i),
                          body.y + static_cast<int>(row), cell.ch, cell.style);
                }
            }
        }
    }
}

void drawFooter(Screen& s, App& app, const Rect& area) {
    Theme theme = app.options.theme;
    Style bg = baseStyle(theme);
    s.fill(area, U' ', bg);

    Rect rows[4];
    for (int i = 0; i < 4; ++i) {
        rows[i] = Rect{area.x, area.y + i, area.w, 1};
    }

    Style statusStyle{themeAccent(theme), themeBG(theme), false};
    std::string status = app.statusLine() + "  (F7 / Ctrl+L toggles)";
    if (app.hasMsg) {
        statusStyle = Style{themeHeaderFG(theme), themeBG(theme), true};
        status = app.statusLine();
    }
    if (0 < area.h) {
        s.putLimit(rows[0].x, rows[0].y, status, statusStyle, area.w);
    }
    if (1 < area.h) {
        s.putLimit(rows[1].x, rows[1].y, app.fkeyLine(),
                   Style{themeDim(theme), themeBG(theme), false}, area.w);
    }
    if (2 < area.h) {
        drawMenuLine(s, app, rows[2].x, rows[2].y);
    }
    if (3 < area.h) {
        s.putLimit(rows[3].x, rows[3].y, app.menuDescription(), bg, area.w);
    }
}

void drawAdaptive(Screen& s, App& app, const Rect& area) {
    int footerH = kFooterRows;
    if (footerH > area.h) {
        footerH = area.h;
    }
    if (int third = area.h / 3; footerH > maxInt(third, 1)) {
        footerH = maxInt(third, 1);
    }
    int bodyH = area.h - footerH;
    int middleH = bodyH >= 12 ? 2 : 0;
    int panesH = bodyH - middleH;
    int topH = panesH / 2;
    int bottomH = panesH - topH;
    Rect top{area.x, area.y, area.w, topH};
    Rect middle{area.x, area.y + topH, area.w, middleH};
    Rect bottom{area.x, area.y + topH + middleH, area.w, bottomH};
    Rect footer{area.x, area.y + bodyH, area.w, footerH};

    int bytesPerRow = adaptiveBytesPerRowLocal(area.w);
    int rows = satSub(topH, 1);
    if (int r = satSub(bottomH, 1); r < rows) {
        rows = r;
    }
    app.syncGeometry(rows, bytesPerRow);

    drawPane(s, app, 0, top);
    drawPane(s, app, 1, bottom);
    if (middleH > 0) {
        drawAdaptiveMiddle(s, app, middle);
    }
    drawFooter(s, app, footer);
}

void drawMenuLine(Screen& s, App& app, int x, int y) {
    Theme theme = app.options.theme;
    int cx = s.put(x, y, "CABLE: ", baseStyle(theme));
    for (std::size_t i = 0; i < kMenuItems.size(); ++i) {
        if (i > 0) {
            s.set(cx, y, U' ', baseStyle(theme));
            ++cx;
        }
        Style base = baseStyle(theme);
        if (static_cast<int>(i) == app.menu) {
            base = Style{themeBarFG(theme), themeBarBG(theme), false};
        }
        std::string label = kMenuItems[i].label;
        auto runes = utf8Decode(label);
        if (!runes.empty()) {
            Style boldBase = base;
            boldBase.bold = true;
            s.set(cx, y, runes[0], boldBase);
            ++cx;
            for (std::size_t k = 1; k < runes.size(); ++k) {
                s.set(cx, y, runes[k], base);
                ++cx;
            }
        }
    }
}

// Forward declarations for helpers used above.

Rect drawBox(Screen& s, const Rect& r, std::string_view title, Theme theme) {
    Style fill = baseStyle(theme);
    Style border{themeBorder(theme), themeBG(theme), false};
    s.fill(r, U' ', fill);
    if (r.w < 2 || r.h < 2) {
        return Rect{r.x, r.y, maxInt(r.w - 2, 0), maxInt(r.h - 2, 0)};
    }
    int top = r.y;
    int bottom = r.y + r.h - 1;
    int left = r.x;
    int right = r.x + r.w - 1;
    for (int x = left + 1; x < right; ++x) {
        s.set(x, top, U'\u2500', border);
        s.set(x, bottom, U'\u2500', border);
    }
    for (int y = top + 1; y < bottom; ++y) {
        s.set(left, y, U'\u2502', border);
        s.set(right, y, U'\u2502', border);
    }
    s.set(left, top, U'\u250C', border);
    s.set(right, top, U'\u2510', border);
    s.set(left, bottom, U'\u2514', border);
    s.set(right, bottom, U'\u2518', border);
    if (!title.empty()) {
        s.putLimit(left + 1, top, title, fill, maxInt(r.w - 2, 0));
    }
    return Rect{r.x + 1, r.y + 1, maxInt(r.w - 2, 0), maxInt(r.h - 2, 0)};
}

Rect centered(const Rect& area, int width, int height) {
    int w = width;
    if (w > area.w) {
        w = area.w;
    }
    if (w < 3) {
        w = 3;
    }
    int h = height;
    if (h > area.h) {
        h = area.h;
    }
    if (h < 3) {
        h = 3;
    }
    return Rect{area.x + satSub(area.w, w) / 2, area.y + satSub(area.h, h) / 2, w, h};
}

void drawDirImpl(Screen& s, DirBrowser& b, Theme theme, const Rect& area) {
    Rect rect = centered(area, satSub(area.w, 4), satSub(area.h, 2));
    Rect inner = drawBox(s, rect, "Dir: " + b.dir().string(), theme);
    if (inner.h < 4 || inner.w == 0) {
        return;
    }

    std::string status = "DIR: Enter [drive:][path\\][filespec] (press <key> for current directory)";
    s.putLimit(inner.x, inner.y, status, Style{themeAccent(theme), themeBG(theme), false}, inner.w);

    const int footerRowCount = 3;
    int listH = satSub(inner.h, 1 + footerRowCount);
    b.ensureVisible(maxInt(listH, 1));
    Rect listRect{inner.x, inner.y + 1, inner.w, listH};
    const auto& entries = b.entries();
    for (int i = 0; i < listH; ++i) {
        int idx = b.scroll() + i;
        if (idx >= static_cast<int>(entries.size())) {
            break;
        }
        const DirEntry& e = entries[static_cast<std::size_t>(idx)];
        std::string size = e.isDir ? "<DIR>" : formatSize(e.size);
        std::string mark = e.isDir ? "/" : " ";
        std::string line =
            strprintf("%s%-36s %9s  %s", mark.c_str(), e.name.c_str(), size.c_str(),
                      formatTime(e.modified, e.hasTime).c_str());
        Style st = baseStyle(theme);
        if (idx == b.selected()) {
            st = Style{themeBG(theme), themeCursorBG(theme), true};
        }
        s.putLimit(listRect.x, listRect.y + i, line, st, listRect.w);
    }

    struct FooterLine {
        std::string text;
        Style style;
    };
    std::vector<FooterLine> footer;
    if (b.pathInput()) {
        footer.push_back(
            FooterLine{"Path: " + *b.pathInput() + "_", Style{themeHeaderFG(theme), themeBG(theme), false}});
    } else if (b.sortPrompt()) {
        footer.push_back(FooterLine{"Sort by N(ame), E(xtension), D(ate) or S(ize)?",
                                    Style{themeHeaderFG(theme), themeBG(theme), true}});
    } else {
        footer.push_back(FooterLine{
            "Scroll using <keys>, PgUp, PgDn, Home, End  -  Esc returns to CABLE",
            Style{themeDim(theme), themeBG(theme), false}});
        int selectedDisplay = entries.empty() ? 0 : b.selected() + 1;
        footer.push_back(FooterLine{
            strprintf("%s   %d of %d   Enter = open   Ctrl+Enter = view in other window   Tab = "
                      "target   S = sort   / = path",
                      sortLabel(b.sort()), selectedDisplay, static_cast<int>(entries.size())),
            Style{themeDim(theme), themeBG(theme), false}});
    }
    if (!b.error().empty()) {
        footer.push_back(FooterLine{b.error(), Style{themeDiffBG(theme), themeBG(theme), false}});
    }
    int footerY = inner.y + inner.h - static_cast<int>(footer.size());
    if (footerY < inner.y) {
        footerY = inner.y;
    }
    for (std::size_t i = 0; i < footer.size(); ++i) {
        if (inner.y + inner.h - 1 < footerY + static_cast<int>(i)) {
            break;
        }
        s.putLimit(inner.x, footerY + static_cast<int>(i), footer[i].text, footer[i].style,
                   inner.w);
    }
}

void drawModalImpl(Screen& s, Modal& m, Options& options, Theme theme, const Rect& area) {
    switch (m.kind) {
    case ModalKind::Confirm: {
        Rect rect = centered(area, 76, 7);
        Rect inner = drawBox(s, rect, m.title, theme);
        std::vector<std::string> lines{""};
        auto wrapped = wrapText(m.text, inner.w);
        lines.insert(lines.end(), wrapped.begin(), wrapped.end());
        lines.push_back("");
        lines.push_back("Y = yes    N = no    Esc = cancel");
        int y = inner.y;
        for (std::size_t i = 0; i < lines.size(); ++i) {
            if (i == 1) {
                s.putLimit(inner.x, y, lines[i], Style{themeHeaderFG(theme), themeBG(theme), true},
                           inner.w);
            } else if (i == lines.size() - 1) {
                s.putLimit(inner.x, y, lines[i], Style{themeDim(theme), themeBG(theme), false},
                           inner.w);
            }
            ++y;
            if (y >= inner.y + inner.h) {
                break;
            }
        }
        break;
    }
    case ModalKind::Input: {
        Rect rect = centered(area, 78, 8);
        Rect inner = drawBox(s, rect, m.title, theme);
        std::string hint;
        switch (m.input) {
        case InputAction::Add:
        case InputAction::Sub:
        case InputAction::Goto:
            hint = "Decimal, or HEX with a leading H or 0x";
            break;
        case InputAction::Search:
            hint = "Enter = search    Esc = cancel";
            break;
        default:
            hint = "Enter = open    Esc = cancel";
            break;
        }
        s.putLimit(inner.x, inner.y, m.prompt, Style{themeAccent(theme), themeBG(theme), false},
                   inner.w);
        s.put(inner.x, inner.y + 2, "> ", Style{themeAccent(theme), themeBG(theme), false});
        int valueX = inner.x + 2;
        s.putLimit(valueX, inner.y + 2, m.value,
                   Style{themeCursorFG(theme), themeCursorBG(theme), false},
                   maxInt(inner.w - 3, 0));
        valueX += static_cast<int>(runeLen(m.value));
        if (valueX < inner.x + inner.w) {
            s.set(valueX, inner.y + 2, U'_', baseStyle(theme));
        }
        s.putLimit(inner.x, inner.y + 4, hint, Style{themeDim(theme), themeBG(theme), false},
                   inner.w);
        break;
    }
    case ModalKind::Options: {
        Rect rect = centered(area, 64, 15);
        Rect inner = drawBox(s, rect, "OPTIONS", theme);
        struct Label {
            const char* name;
            bool current;
        };
        Label labels[] = {
            {"ASCII", !options.ebcdic},
            {"Back", !options.forward},
            {"Conceal", options.conceal},
            {"EBCDIC", options.ebcdic},
            {"Fore", options.forward},
            {"Help", false},
            {"Ignore", !options.respectCase},
            {"Quit", false},
            {"Respect", options.respectCase},
            {"Show", !options.conceal},
            {"heX/char", options.hexInput},
        };
        for (int i = 0; i < 11 && i < inner.h; ++i) {
            bool selected = i == m.index;
            Style st = baseStyle(theme);
            if (selected) {
                st = Style{themeBG(theme), themeCursorBG(theme), true};
            }
            std::string mark = labels[i].current ? "(*)" : "   ";
            s.putLimit(inner.x, inner.y + i,
                       strprintf("  %-24s %s", labels[i].name, mark.c_str()), st, inner.w);
        }
        if (11 < inner.h) {
            s.putLimit(inner.x, inner.y + 11, "Enter/Space = select   Esc = close",
                       Style{themeDim(theme), themeBG(theme), false}, inner.w);
        }
        break;
    }
    case ModalKind::Modes: {
        Rect rect = centered(area, 44, 9);
        Rect inner = drawBox(s, rect, "MODE", theme);
        const char* labels[] = {"Attributes", "Color", "Help", "Mono", "Quit"};
        for (int i = 0; i < 5 && i < inner.h; ++i) {
            Style st = baseStyle(theme);
            if (i == m.index) {
                st = Style{themeBG(theme), themeCursorBG(theme), true};
            }
            s.putLimit(inner.x, inner.y + i, strprintf("  %-24s", labels[i]), st, inner.w);
        }
        break;
    }
    case ModalKind::Attributes: {
        Rect rect = centered(area, 46, 8);
        Rect inner = drawBox(s, rect, "ATTRIBUTES", theme);
        Theme themes[] = {Theme::Classic, Theme::Amber, Theme::Mono};
        for (int i = 0; i < 3 && i < inner.h; ++i) {
            std::string text = themeName(themes[i]);
            if (themes[i] == theme) {
                text += "  (current)";
            }
            Style st = baseStyle(theme);
            if (i == m.index) {
                st = Style{themeBG(theme), themeCursorBG(theme), true};
            }
            s.putLimit(inner.x, inner.y + i, strprintf("  %-24s", text.c_str()), st, inner.w);
        }
        if (3 < inner.h) {
            s.putLimit(inner.x, inner.y + 3, "Enter = choose   Esc = close",
                       Style{themeDim(theme), themeBG(theme), false}, inner.w);
        }
        break;
    }
    case ModalKind::Help: {
        Rect rect = centered(area, 88, satSub(area.h, 2));
        Rect inner = drawBox(s, rect, kHelpTitle, theme);
        int visible = inner.h;
        int max = static_cast<int>(kHelpLines.size()) - 1;
        int start = m.scroll;
        if (start > max) {
            start = max;
        }
        if (start < 0) {
            start = 0;
        }
        int end = start + visible;
        if (end > static_cast<int>(kHelpLines.size())) {
            end = static_cast<int>(kHelpLines.size());
        }
        for (int i = start; i < end; ++i) {
            s.putLimit(inner.x, inner.y + (i - start), kHelpLines[static_cast<std::size_t>(i)],
                       baseStyle(theme), inner.w);
        }
        if (inner.h > 0) {
            std::string footer = strprintf(
                "Line %d of %d   Up/Down, PgUp/PgDn, Home/End scroll   Esc close", start + 1,
                static_cast<int>(kHelpLines.size()));
            s.putLimit(inner.x, inner.y + inner.h - 1, footer,
                       Style{themeDim(theme), themeBG(theme), false}, inner.w);
        }
        break;
    }
    case ModalKind::Dir:
        if (m.dir) {
            drawDirImpl(s, *m.dir, theme, area);
        }
        break;
    }
}

} // namespace

int adaptiveBytesPerRow(int width) { return adaptiveBytesPerRowLocal(width); }

int adaptiveHexCenter(int bytesPerRow, int dataCol) {
    int groups = bytesPerRow / 16;
    if (groups < 1) {
        groups = 1;
    }
    return dataCol + (33 * groups - 1) / 2;
}

std::vector<std::vector<Cell>> disasmWindow(App& app, int index, int rows, int width) {
    Theme theme = app.options.theme;
    Style normal = baseStyle(theme);
    Style currentStyle = normal;
    currentStyle.bold = true;
    Style diffStyle{themeDiffFG(theme), themeDiffBG(theme), false};

    std::vector<std::vector<Cell>> out;
    out.reserve(static_cast<std::size_t>(std::max(rows, 0)));
    for (int row = 0; row < rows; ++row) {
        std::vector<Cell> line;
        line.reserve(static_cast<std::size_t>(std::max(width, 0)));
        for (int i = 0; i < width; ++i) {
            line.push_back(Cell{U' ', normal});
        }
        out.push_back(std::move(line));
    }
    if (!app.options.disasm || rows == 0 || width == 0 || !app.panes[index].file) {
        return out;
    }

    bool compare = app.compareMode;
    u64 cursor = app.panes[index].cursor;
    int center = rows / 2;

    u64 anchor = 0;
    if (cursor > 512) {
        anchor = ((cursor - 512) / 256) * 256;
    }
    u64 end = cursor + 512;
    if (end > app.panes[index].length()) {
        end = app.panes[index].length();
    }
    std::vector<u8> bytes;
    bytes.reserve(static_cast<std::size_t>(end - anchor));
    for (u64 off = anchor; off < end; ++off) {
        auto b = byteAt(app, index, off);
        if (!b) {
            break;
        }
        bytes.push_back(*b);
    }
    auto insns = disassemble(*app.options.disasm, bytes);
    if (insns.empty()) {
        return out;
    }

    long long rel = static_cast<long long>(cursor - anchor);
    std::size_t ci = 0;
    for (std::size_t i = 0; i < insns.size(); ++i) {
        if (insns[i].offset <= rel) {
            ci = i;
        }
    }
    bool onInstruction = rel >= insns[ci].offset && rel < insns[ci].offset + insns[ci].len;

    for (int row = 0; row < rows; ++row) {
        long long target = static_cast<long long>(row - center);
        long long idx = static_cast<long long>(ci) + target;
        if (idx < 0 || idx >= static_cast<long long>(insns.size())) {
            continue;
        }
        const Insn& insn = insns[static_cast<std::size_t>(idx)];
        u64 abs = anchor + static_cast<u64>(insn.offset);
        bool isCursor = onInstruction && static_cast<std::size_t>(idx) == ci;
        bool differs = false;
        if (compare) {
            for (int k = 0; k < insn.len; ++k) {
                u64 off = abs + static_cast<u64>(k);
                auto a = byteAt(app, index, off);
                auto b = byteAt(app, 1 - index, off);
                if (a && b) {
                    if (*a != *b) {
                        differs = true;
                        break;
                    }
                } else if (static_cast<bool>(a) != static_cast<bool>(b)) {
                    differs = true;
                    break;
                }
            }
        }
        Style style = normal;
        if (isCursor) {
            style = currentStyle;
        } else if (differs) {
            style = diffStyle;
        }
        std::string text = insn.text;
        if (width >= 24) {
            text = strprintf("%08llX  %s", static_cast<unsigned long long>(abs), text.c_str());
        }
        auto r = utf8Decode(text);
        for (int i = 0; i < width && i < static_cast<int>(r.size()); ++i) {
            out[static_cast<std::size_t>(row)][static_cast<std::size_t>(i)] =
                Cell{r[static_cast<std::size_t>(i)], style};
        }
    }
    return out;
}

void drawModal(Screen& screen, Modal& modal, Options& options, Theme theme, const Rect& area) {
    drawModalImpl(screen, modal, options, theme, area);
}

void drawDir(Screen& screen, DirBrowser& browser, Theme theme, const Rect& area) {
    drawDirImpl(screen, browser, theme, area);
}

void drawUI(Screen& screen, App& app) {
    Theme theme = app.options.theme;
    if (screen.w == kOriginalCols && screen.h == kOriginalRows) {
        drawOriginal(screen, app);
    } else {
        drawAdaptive(screen, app, Rect{0, 0, screen.w, screen.h});
    }
    if (app.modal) {
        drawModal(screen, *app.modal, app.options, theme, Rect{0, 0, screen.w, screen.h});
    }
}
