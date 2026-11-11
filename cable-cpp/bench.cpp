// Non-interactive I/O benchmark shared with the Rust and Go ports.

#include "cable.hpp"

#include <cinttypes>
#include <cstdio>

namespace {

using Clock = std::chrono::steady_clock;

double elapsedMs(Clock::time_point start) {
    auto end = Clock::now();
    return std::chrono::duration<double, std::milli>(end - start).count();
}

struct Fnv {
    u64 value = 14695981039346656037ULL;

    void update(const u8* data, std::size_t n) {
        for (std::size_t i = 0; i < n; ++i) {
            value ^= data[i];
            value *= 1099511628211ULL;
        }
    }
};

void emit(const char* phase, double ms, u64 bytes, u64 checksum) {
    std::printf("%s,%.3f,%llu,%llu\n", phase, ms, static_cast<unsigned long long>(bytes),
                static_cast<unsigned long long>(checksum));
    std::fflush(stdout);
}

std::vector<u8> lastBytes(PagedFile& file, std::size_t want) {
    u64 length = file.length();
    u64 start = length > want ? length - want : 0;
    return file.readRange(start, static_cast<std::size_t>(length - start));
}

std::optional<std::vector<u8>> locateNeedle(PagedFile& file) {
    const std::string marker = "CABLE-NEEDLE-1234";
    auto tail = lastBytes(file, 4096);
    auto it = std::search(tail.begin(), tail.end(), marker.begin(), marker.end());
    if (it == tail.end()) {
        return std::nullopt;
    }
    return std::vector<u8>(it, it + static_cast<std::ptrdiff_t>(marker.size()));
}

} // namespace

int runBenchmark(const std::vector<fs::path>& paths) {
    if (paths.empty() || paths.size() > 2) {
        std::fprintf(stderr, "usage: cable --bench FILE1 [FILE2]\n");
        return 2;
    }

    std::printf("phase,milliseconds,bytes,checksum\n");

    // open
    std::string error;
    auto openStart = Clock::now();
    auto file1 = PagedFile::open(paths[0], error);
    double openMs = elapsedMs(openStart);
    if (!file1) {
        std::fprintf(stderr, "cable: %s\n", error.c_str());
        return 1;
    }
    u64 length1 = file1->length();
    emit("open", openMs, length1, length1);

    // scan
    Fnv checksum;
    u64 scanned = 0;
    auto scanStart = Clock::now();
    {
        const std::size_t chunk = 64 * 1024;
        u64 pos = 0;
        while (pos < length1) {
            std::size_t want = static_cast<std::size_t>(std::min<u64>(length1 - pos, chunk));
            auto data = file1->readRange(pos, want);
            if (data.empty()) {
                break;
            }
            checksum.update(data.data(), data.size());
            scanned += data.size();
            pos += data.size();
        }
    }
    emit("scan", elapsedMs(scanStart), scanned, checksum.value);

    // search
    auto needle = locateNeedle(*file1);
    auto searchStart = Clock::now();
    std::optional<u64> found;
    if (needle) {
        found = findBytes(*file1, *needle, 0, true, true, false);
    }
    double searchMs = elapsedMs(searchStart);
    emit("search", searchMs, length1, found ? *found : UINT64_MAX);

    // compare
    std::unique_ptr<PagedFile> file2;
    u64 compareBytes = 0;
    std::optional<u64> difference;
    if (paths.size() >= 2) {
        file2 = PagedFile::open(paths[1], error);
        if (!file2) {
            std::fprintf(stderr, "cable: %s\n", error.c_str());
            return 1;
        }
        compareBytes = std::min(file1->length(), file2->length());
        auto compareStart = Clock::now();
        difference = findDifference(*file1, *file2, 0, true);
        emit("compare", elapsedMs(compareStart), compareBytes,
             difference ? *difference : UINT64_MAX);
    } else {
        emit("compare", 0.0, 0, UINT64_MAX);
    }

    // save: copy the input to a temporary file outside the timed region, then
    // apply one edit and flush the rewritten copy to disk.
    fs::path temp = fs::path(paths[0]).parent_path() /
                    strprintf(".cable-bench-%llu.tmp",
                              static_cast<unsigned long long>(
                                  std::chrono::duration_cast<std::chrono::nanoseconds>(
                                      std::chrono::system_clock::now().time_since_epoch())
                                      .count()));
    std::error_code ec;
    fs::copy_file(paths[0], temp, fs::copy_options::overwrite_existing, ec);
    if (ec) {
        std::fprintf(stderr, "cable: could not create benchmark copy: %s\n", ec.message().c_str());
        return 1;
    }
    auto saveFile = PagedFile::open(temp, error);
    if (!saveFile) {
        fs::remove(temp, ec);
        std::fprintf(stderr, "cable: %s\n", error.c_str());
        return 1;
    }
    u64 saveBytes = saveFile->length();
    auto saveStart = Clock::now();
    u8 original = 0;
    if (saveBytes > 0) {
        auto value = saveFile->readOffset(0);
        original = value ? *value : 0;
    }
    std::string saveError;
    if (!saveFile->writeOffset(0, static_cast<u8>(original ^ 0xFF), saveError)) {
        saveFile.reset();
        fs::remove(temp, ec);
        std::fprintf(stderr, "cable: %s\n", saveError.c_str());
        return 1;
    }
    u64 savedLength = saveFile->save(saveError);
    double saveMs = elapsedMs(saveStart);
    if (!saveError.empty()) {
        saveFile.reset();
        fs::remove(temp, ec);
        std::fprintf(stderr, "cable: %s\n", saveError.c_str());
        return 1;
    }
    emit("save", saveMs, saveBytes, savedLength);
    saveFile.reset();
    fs::remove(temp, ec);

    return 0;
}
