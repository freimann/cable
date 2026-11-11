CXX ?= c++
CXXFLAGS ?= -std=c++20 -O3 -DNDEBUG -Wall -Wextra

SOURCES = core.cpp ui.cpp app.cpp key.cpp term_posix.cpp term_windows.cpp main.cpp bench.cpp
HEADERS = cable.hpp tables.hpp help_text.hpp

cable: $(SOURCES) $(HEADERS)
	$(CXX) $(CXXFLAGS) -o $@ $(SOURCES)

test: cable
	sh tests/smoke.sh ./cable

clean:
	rm -f cable

.PHONY: test clean
