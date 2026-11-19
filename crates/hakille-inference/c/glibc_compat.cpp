#include <cstdlib>
#include <clocale>
#include <cstring>
#include <cstddef>

// Compatibility shims for systems running glibc < 2.38 and GCC < 13
// where precompiled ONNX Runtime static/dynamic symbols might reference ISO C23
// conversion functions and newer libstdc++ cold string replacement routines.
// Marked weak so standard system libraries take precedence when present.

namespace {
template <typename CharT>
inline void replace_cold_impl(
    CharT *p,
    std::size_t len1,
    const CharT *s,
    std::size_t len2,
    std::size_t how_much
) noexcept {
    if (len2 && len2 <= len1) {
        std::memmove(p, s, len2 * sizeof(CharT));
    }
    if (how_much && len1 != len2) {
        std::memmove(p + len2, p + len1, how_much * sizeof(CharT));
    }
    if (len2 > len1) {
        if (s + len2 <= p + len1) {
            std::memmove(p, s, len2 * sizeof(CharT));
        } else if (s >= p + len1) {
            std::memmove(p, s + len2 - len1, len2 * sizeof(CharT));
        } else {
            const std::size_t nleft = static_cast<std::size_t>((p + len1) - s);
            std::memmove(p, s, nleft * sizeof(CharT));
            std::memmove(p + nleft, p + len2, (len2 - nleft) * sizeof(CharT));
        }
    }
}
} // namespace

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

__attribute__((weak)) void _ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE15_M_replace_coldEPcmPKcmm(
    void * /*this_ptr*/,
    char *p,
    std::size_t len1,
    const char *s,
    std::size_t len2,
    std::size_t how_much
) {
    replace_cold_impl<char>(p, len1, s, len2, how_much);
}

__attribute__((weak)) void _ZNSt7__cxx1112basic_stringIwSt11char_traitsIwESaIwEE15_M_replace_coldEPwmPKwmm(
    void * /*this_ptr*/,
    wchar_t *p,
    std::size_t len1,
    const wchar_t *s,
    std::size_t len2,
    std::size_t how_much
) {
    replace_cold_impl<wchar_t>(p, len1, s, len2, how_much);
}
}
