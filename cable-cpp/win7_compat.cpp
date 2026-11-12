// Definitions for the declarations in win7_compat.h.  Only used by the
// Windows 7 (msvcrt) build; the Universal CRT provides the real functions.
#include "win7_compat.h"

#if defined(_WIN32) && !defined(_UCRT)
#include <cstdlib>

extern "C" void _exit(int);

extern "C" void quick_exit(int code) { _exit(code); }
extern "C" int at_quick_exit(void (*fn)(void)) { return std::atexit(fn); }
#endif
