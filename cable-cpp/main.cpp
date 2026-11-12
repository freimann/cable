// CABLE-CPP entry point and option parsing.

#include "cable.hpp"

#include <cstdio>
#include <cstdlib>

namespace {

const char* kVersion = "0.2.4";

void usage() {
    const char* body = R"( - Compare And Browse with Limited Editing

Usage: cable [OPTIONS] [FILE1 [FILE2]]
       cable --bench FILE1 [FILE2]

With no files, a directory browser is opened.

Options:
  --mono            start in mono
  --colour, --color start in colour (default)
  --amber           start in amber
  --ascii           treat data as ASCII (default)
  --ebcdic          treat data as EBCDIC (CP037)
  --conceal         show unprintables as '.' (default)
  --show            show unprintables as control pictures
  --respect-case    case-sensitive search/compare (default)
  --ignore-case     case-insensitive search/compare
  --hex             HEX search/edit input
  --character       character search/edit input (default)
  --forward         compare/search forwards (default)
  --backward        compare/search backwards
  --active          scroll only the active window (default)
  --both            scroll both windows together
  --text            show ASCII/EBCDIC in the right-hand column (default)
  --x86, --x64      disassemble the right-hand column as x86 / x86-64
  --arm, --thumb    disassemble as ARMv7 / Thumb
  --arm64           disassemble as AArch64
  --disasm=CPU      same as the flags above (CPU: x86, x64, arm, thumb, arm64)
  --bench           run the non-interactive I/O benchmark
  -h, --help        show this help
  -V, --version     show version
)";
    std::printf("cable %s (C++ port of cable-rs)%s", kVersion, body);
}

bool startsWith(std::string_view text, std::string_view prefix) {
    return text.size() >= prefix.size() && text.substr(0, prefix.size()) == prefix;
}

} // namespace

int main(int argc, char** argv) {
    std::vector<std::string> args;
    for (int i = 1; i < argc; ++i) {
        args.emplace_back(argv[i]);
    }

    if (!args.empty() && args[0] == "--bench") {
        std::vector<fs::path> paths;
        for (std::size_t i = 1; i < args.size(); ++i) {
            paths.emplace_back(args[i]);
        }
        return runBenchmark(paths);
    }

    Options options = defaultOptions();
    std::vector<fs::path> paths;

    for (const auto& arg : args) {
        if (arg == "-h" || arg == "--help") {
            usage();
            return 0;
        }
        if (arg == "-V" || arg == "--version") {
            std::printf("cable %s (C++)\n", kVersion);
            return 0;
        }
        if (arg == "--mono") {
            options.theme = Theme::Mono;
        } else if (arg == "--colour" || arg == "--color") {
            options.theme = Theme::Classic;
        } else if (arg == "--amber") {
            options.theme = Theme::Amber;
        } else if (arg == "--ebcdic") {
            options.ebcdic = true;
        } else if (arg == "--ascii") {
            options.ebcdic = false;
        } else if (arg == "--show") {
            options.conceal = false;
        } else if (arg == "--conceal") {
            options.conceal = true;
        } else if (arg == "--ignore-case") {
            options.respectCase = false;
        } else if (arg == "--respect-case") {
            options.respectCase = true;
        } else if (arg == "--hex") {
            options.hexInput = true;
        } else if (arg == "--character") {
            options.hexInput = false;
        } else if (arg == "--backward") {
            options.forward = false;
        } else if (arg == "--forward") {
            options.forward = true;
        } else if (arg == "--both") {
            options.moveBoth = true;
        } else if (arg == "--active") {
            options.moveBoth = false;
        } else if (arg == "--x86") {
            options.disasm = Cpu::X86;
        } else if (arg == "--x64" || arg == "--amd64") {
            options.disasm = Cpu::X64;
        } else if (arg == "--arm") {
            options.disasm = Cpu::Arm;
        } else if (arg == "--thumb") {
            options.disasm = Cpu::Thumb;
        } else if (arg == "--arm64" || arg == "--aarch64") {
            options.disasm = Cpu::Arm64;
        } else if (arg == "--text" || arg == "--ascii-view") {
            options.disasm.reset();
        } else if (startsWith(arg, "--disasm=")) {
            std::string name = arg.substr(std::string("--disasm=").size());
            auto cpu = parseCpu(name);
            if (!cpu) {
                usage();
                std::fprintf(stderr, "cable: unknown disassembly target %s\n", name.c_str());
                return 1;
            }
            options.disasm = *cpu;
        } else if (!arg.empty() && arg[0] == '-') {
            usage();
            std::fprintf(stderr, "cable: unknown option %s\n", arg.c_str());
            return 1;
        } else {
            paths.emplace_back(arg);
        }
    }

    if (paths.size() > 2) {
        usage();
        std::fprintf(stderr, "cable: at most two files can be opened\n");
        return 1;
    }

    std::string error;
    auto app = App::create(paths, options, error);
    if (!app) {
        std::fprintf(stderr, "cable: %s\n", error.c_str());
        return 1;
    }

    auto terminal = Terminal::start(error);
    if (!terminal) {
        std::fprintf(stderr, "cable: %s\n", error.c_str());
        return 1;
    }

    KeyReader reader;
    bool dirty = true;
    int lastW = 0;
    int lastH = 0;

    while (!app->quit) {
        if (dirty) {
            int w = 80;
            int h = 25;
            terminalSize(w, h);
            Screen screen(w, h, baseStyle(app->options.theme));
            drawUI(screen, *app);
            std::string out;
            screen.flush(out);
            writeTerminal(out);
            lastW = w;
            lastH = h;
            dirty = false;
        }
        if (auto key = reader.poll(200)) {
            app->handleKey(*key);
            dirty = true;
        } else {
            int w = 80;
            int h = 25;
            terminalSize(w, h);
            if (w != lastW || h != lastH) {
                dirty = true;
            }
        }
    }
    return 0;
}
