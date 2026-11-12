CXX ?= c++
CXXFLAGS ?= -std=c++20 -O3 -DNDEBUG -Wall -Wextra
DIST := dist

SOURCES = core.cpp ui.cpp app.cpp key.cpp term_posix.cpp term_windows.cpp main.cpp bench.cpp
HEADERS = cable.hpp tables.hpp help_text.hpp

# Windows 7 runs the classic msvcrt, not the Universal CRT.  -mcrtdll=msvcrt-os
# selects the old import library, __LARGE_MBSTATE_T matches the mbstate_t the
# prebuilt libstdc++ was compiled against, and win7_compat supplies the C11
# quick-exit functions that msvcrt lacks.
WIN32_CXX ?= i686-w64-mingw32-g++
WIN32_FLAGS = -std=c++20 -O3 -DNDEBUG -static -mcrtdll=msvcrt-os \
	-D__LARGE_MBSTATE_T -include win7_compat.h -Wl,-u,___ms_vsnprintf

cable: $(SOURCES) $(HEADERS)
	$(CXX) $(CXXFLAGS) -o $@ $(SOURCES)

windows-x86: $(SOURCES) $(HEADERS) win7_compat.h win7_compat.cpp
	@mkdir -p $(DIST)
	$(WIN32_CXX) $(WIN32_FLAGS) -o $(DIST)/cable-cpp-windows-x86.exe \
		$(SOURCES) win7_compat.cpp

test: cable
	sh tests/smoke.sh ./cable

clean:
	rm -f cable
	rm -f $(DIST)/cable-cpp-windows-x86.exe

.PHONY: test clean windows-x86
