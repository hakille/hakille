#include <cstdlib>
#include <clocale>

// Compatibility shims for systems running glibc < 2.38 and GCC < 13
// where precompiled ONNX Runtime static/dynamic symbols might reference ISO C23
// conversion functions and newer libstdc++ cold string replacement routines.
// Marked weak so standard system libraries take precedence when present.

extern "C" {
__attribute__((weak)) long int __isoc23_strtol(const char *nptr, char **endptr, int base) {
    return strtol(nptr, endptr, base);
}

__attribute__((weak)) long long int __isoc23_strtoll(const char *nptr, char **endptr, int base) {
    return strtoll(nptr, endptr, base);
}

__attribute__((weak)) long long int __isoc23_strtoll_l(const char *nptr, char **endptr, int base, locale_t loc) {
    return strtoll_l(nptr, endptr, base, loc);
}

__attribute__((weak)) unsigned long int __isoc23_strtoul(const char *nptr, char **endptr, int base) {
    return strtoul(nptr, endptr, base);
}

__attribute__((weak)) unsigned long long int __isoc23_strtoull(const char *nptr, char **endptr, int base) {
    return strtoull(nptr, endptr, base);
}

__attribute__((weak)) unsigned long long int __isoc23_strtoull_l(const char *nptr, char **endptr, int base, locale_t loc) {
    return strtoull_l(nptr, endptr, base, loc);
}

__attribute__((weak)) void _ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE15_M_replace_coldEPcmPKcmm() {}
__attribute__((weak)) void _ZNSt7__cxx1112basic_stringIwSt11char_traitsIwESaIwEE15_M_replace_coldEPwmPKwmm() {}
}
