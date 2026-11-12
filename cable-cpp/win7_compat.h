// Windows 7 (msvcrt) build shim.  mingw-w64 only declares the C11
// quick-exit functions when targeting the Universal CRT, but the prebuilt
// libstdc++ headers reference them unconditionally.  Force-include this
// header before any standard header when building with -mcrtdll=msvcrt-os.
#pragma once

#if defined(_WIN32) && !defined(_UCRT)
#ifdef __cplusplus
extern "C" {
#endif
void quick_exit(int) __attribute__((noreturn));
int at_quick_exit(void (*)(void));
#ifdef __cplusplus
}
#endif
#endif
