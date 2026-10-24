/*
 *
 * Copyright (c) 2004
 * John Maddock
 *
 * Use, modification and distribution are subject to the 
 * Boost Software License, Version 1.0. (See accompanying file 
 * LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org for most recent version.
  *   FILE         w32_regex_traits.hpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Declares regular expression traits class w32_regex_traits.
  */

#ifndef BOOST_W32_REGEX_TRAITS_HPP_INCLUDED
#define BOOST_W32_REGEX_TRAITS_HPP_INCLUDED

#include <boost/regex/pattern_except.hpp>
#include <boost/regex/v5/regex_traits_defaults.hpp>
#include <boost/regex/v5/primary_transform.hpp>
#include <boost/regex/v5/object_cache.hpp>

#ifdef BOOST_REGEX_MSVC
#pragma warning(push)
#pragma warning(disable : 4786)
#if BOOST_REGEX_MSVC < 1910
#pragma warning(disable : 4800)
#endif
#endif

namespace boost {

//
// forward declaration is needed by some compilers:
//
template <class charT> class w32_regex_traits;

namespace BOOST_REGEX_DETAIL_NS {

//
// start by typedeffing the types we'll need:
//
typedef unsigned long lcid_type; // placeholder for LCID.

//
// then add wrappers around the actual Win32 API's (ie implementation hiding):
//
lcid_type w32_get_default_locale();
std::string w32_transform(lcid_type state_id, const char *p1, const char *p2);
char w32_toupper(char c, lcid_type);
bool w32_is(lcid_type, std::uint32_t mask, char c);

#ifndef BASETYPES
//
// Forward declarations of the small number of windows types and API's we use:
//

#if !defined(__LP64__)
using dword = unsigned long;
#else
using DWORD = unsigned int;
#endif
using word = unsigned short;
using lctype = dword;

static constexpr dword ct_ctype1 = 0x00000001;
static constexpr dword c1_upper = 0x0001;   // upper case
static constexpr dword c1_lower = 0x0002;   // lower case
static constexpr dword c1_digit = 0x0004;   // decimal digits
static constexpr dword c1_space = 0x0008;   // spacing characters
static constexpr dword c1_punct = 0x0010;   // punctuation characters
static constexpr dword c1_cntrl = 0x0020;   // control characters
static constexpr dword c1_blank = 0x0040;   // blank characters
static constexpr dword c1_xdigit = 0x0080;  // other digits
static constexpr dword c1_alpha = 0x0100;   // any linguistic character
static constexpr dword c1_defined = 0x0200; // defined character
static constexpr unsigned int cp_acp = 0;
static constexpr dword lcmap_lowercase = 0x00000100;
static constexpr dword lcmap_uppercase = 0x00000200;
static constexpr dword lcmap_sortkey = 0x00000400; // WC sort key (normalize)
static constexpr lctype locale_idefaultansicodepage = 0x00001004;

#if defined(_M_IX86) || defined(__i386__)
#define BOOST_RE_STDCALL __stdcall
#else
// On architectures other than 32-bit x86 __stdcall is ignored. Clang also issues a warning.
#define BOOST_RE_STDCALL
#endif

#define BOOST_RE_IMPORT __declspec(dllimport)

extern "C" {

BOOST_RE_IMPORT int BOOST_RE_STDCALL LCMapStringA(lcid_type Locale,
                                                  dword dwMapFlags,
                                                  const char *lpSrcStr,
                                                  int cchSrc, char *lpDestStr,
                                                  int cchDest);
BOOST_RE_IMPORT int BOOST_RE_STDCALL GetStringTypeExA(lcid_type Locale,
                                                      dword dwInfoType,
                                                      const char *lpSrcStr,
                                                      int cchSrc,
                                                      word *lpCharType);
BOOST_RE_IMPORT lcid_type BOOST_RE_STDCALL GetUserDefaultLCID();
BOOST_RE_IMPORT int BOOST_RE_STDCALL GetStringTypeExA(lcid_type Locale,
                                                      dword dwInfoType,
                                                      const char *lpSrcStr,
                                                      int cchSrc,
                                                      word *lpCharType);
}

#else
//
// We have windows.h already included:
//
using dword = DWORD;
using word = WORD;
using lctype = LCTYPE;

static constexpr dword ct_ctype1 = 0x00000001;
static constexpr dword c1_upper = 0x0001;   // upper case
static constexpr dword c1_lower = 0x0002;   // lower case
static constexpr dword c1_digit = 0x0004;   // decimal digits
static constexpr dword c1_space = 0x0008;   // spacing characters
static constexpr dword c1_punct = 0x0010;   // punctuation characters
static constexpr dword c1_cntrl = 0x0020;   // control characters
static constexpr dword c1_blank = 0x0040;   // blank characters
static constexpr dword c1_xdigit = 0x0080;  // other digits
static constexpr dword c1_alpha = 0x0100;   // any linguistic character
static constexpr dword c1_defined = 0x0200; // defined character
static constexpr unsigned int cp_acp = 0;
static constexpr dword lcmap_lowercase = 0x00000100;
static constexpr dword lcmap_uppercase = 0x00000200;
static constexpr dword lcmap_sortkey = 0x00000400; // WC sort key (normalize)
static constexpr lctype locale_idefaultansicodepage = 0x00001004;

using ::LCMapStringA;
using ::GetStringTypeExA;
using ::GetUserDefaultLCID;
using ::GetStringTypeExA;

#endif
//
// class w32_regex_traits_base:
// acts as a container for locale and the facets we are using.
//
template <class charT> struct w32_regex_traits_base {
  w32_regex_traits_base(lcid_type l) { imbue(l); }
  lcid_type imbue(lcid_type l);

  lcid_type m_locale;
};

template <class charT>
inline lcid_type w32_regex_traits_base<charT>::imbue(lcid_type l) {
  lcid_type result(m_locale);
  m_locale = l;
  return result;
}

//
// class w32_regex_traits_char_layer:
// implements methods that require specialisation for narrow characters:
//
template <class charT> class w32_regex_traits_char_layer;

template <>
class w32_regex_traits_char_layer<char> : public w32_regex_traits_base<char> {
public:
  w32_regex_traits_char_layer(::boost::BOOST_REGEX_DETAIL_NS::lcid_type l)
      : w32_regex_traits_base<char>(l) {
    init<char>();
  }

  regex_constants::syntax_type syntax_type(char c) const {
    return m_char_map[static_cast<unsigned char>(c)];
  }
  regex_constants::escape_syntax_type escape_syntax_type(char c) const {
    return m_char_map[static_cast<unsigned char>(c)];
  }
  char tolower(char c) const {
    return m_lower_map[static_cast<unsigned char>(c)];
  }
  bool isctype(std::uint32_t mask, char c) const {
    return m_type_map[static_cast<unsigned char>(c)] & mask;
  }

private:
  regex_constants::syntax_type m_char_map[1u << CHAR_BIT];
  char m_lower_map[1u << CHAR_BIT];
  std::uint16_t m_type_map[1u << CHAR_BIT];
  template <class U> void init();
};

//
// class w32_regex_traits_implementation:
// provides pimpl implementation for w32_regex_traits.
//
template <class charT>
class w32_regex_traits_implementation
    : public w32_regex_traits_char_layer<charT> {
public:
  typedef typename w32_regex_traits<charT>::char_class_type char_class_type;
  static const char_class_type mask_word = 0x0400;    // must be C1_DEFINED << 1
  static const char_class_type mask_unicode = 0x0800; // must be C1_DEFINED << 2
  static const char_class_type mask_horizontal =
      0x1000; // must be C1_DEFINED << 3
  static const char_class_type mask_vertical =
      0x2000; // must be C1_DEFINED << 4
  static const char_class_type mask_base =
      0x3ff; // all the masks used by the CT_CTYPE1 group

  typedef std::basic_string<charT> string_type;
  typedef charT char_type;
  w32_regex_traits_implementation(::boost::BOOST_REGEX_DETAIL_NS::lcid_type l);
  std::string error_string(regex_constants::error_type n) const {
    return get_default_error_string(n);
  }
  char_class_type lookup_classname(const charT *p1, const charT *p2) const {
    char_class_type result = lookup_classname_imp(p1, p2);
    if (result == 0) {
      typedef typename string_type::size_type size_type;
      string_type temp(p1, p2);
      for (size_type i = 0; i < temp.size(); ++i)
        temp[i] = this->tolower(temp[i]);
      result =
          lookup_classname_imp(&*temp.begin(), &*temp.begin() + temp.size());
    }
    return result;
  }
  string_type lookup_collatename(const charT *p1, const charT *p2) const;
  string_type transform_primary(const charT *p1, const charT *p2) const;
  string_type transform(const charT *p1, const charT *p2) const {
    return ::boost::BOOST_REGEX_DETAIL_NS::w32_transform(this->m_locale, p1,
                                                         p2);
  }

private:
  unsigned m_collate_type; // the form of the collation string
  charT m_collate_delim;   // the collation group delimiter
  //
  // helpers:
  //
  char_class_type lookup_classname_imp(const charT *p1, const charT *p2) const;
};

template <class charT>
typename w32_regex_traits_implementation<charT>::string_type
w32_regex_traits_implementation<charT>::transform_primary(
    const charT *p1, const charT *p2) const {
  string_type result;
  //
  // What we do here depends upon the format of the sort key returned by
  // sort key returned by this->transform:
  //
  switch (m_collate_type) {
  case sort_C:
  case sort_unknown:
    // the best we can do is translate to lower case, then get a regular sort key:
    {
      result.assign(p1, p2);
      typedef typename string_type::size_type size_type;
      for (size_type i = 0; i < result.size(); ++i)
        result[i] = this->tolower(result[i]);
      result =
          this->transform(&*result.begin(), &*result.begin() + result.size());
      break;
    }
  case sort_fixed: {
    // get a regular sort key, and then truncate it:
    result.assign(this->transform(p1, p2));
    result.erase(this->m_collate_delim);
    break;
  }
  case sort_delim:
    // get a regular sort key, and then truncate everything after the delim:
    result.assign(this->transform(p1, p2));
    std::size_t i;
    for (i = 0; i < result.size(); ++i) {
      if (result[i] == m_collate_delim)
        break;
    }
    result.erase(i);
    break;
  }
  if (result.empty())
    result = string_type(1, charT(0));
  return result;
}

template <class charT>
typename w32_regex_traits_implementation<charT>::string_type
w32_regex_traits_implementation<charT>::lookup_collatename(
    const charT *p1, const charT *p2) const {
  std::string name(p1, p2);
  name = lookup_default_collate_name(name);
  if (name.size())
    return string_type(name.begin(), name.end());
  if (p2 - p1 == 1)
    return string_type(1, *p1);
  return string_type();
}

template <class charT>
w32_regex_traits_implementation<charT>::w32_regex_traits_implementation(
    ::boost::BOOST_REGEX_DETAIL_NS::lcid_type l)
    : w32_regex_traits_char_layer<charT>(l) {
  //
  // get the collation format used by m_pcollate:
  //
  m_collate_type =
      BOOST_REGEX_DETAIL_NS::find_sort_syntax(this, &m_collate_delim);
}

template <class charT>
typename w32_regex_traits_implementation<charT>::char_class_type
w32_regex_traits_implementation<charT>::lookup_classname_imp(
    const charT *p1, const charT *p2) const {
  static const char_class_type masks[22] = {
      0,
      0x0104u, // C1_ALPHA | C1_DIGIT
      0x0100u, // C1_ALPHA
      0x0040u, // C1_BLANK
      0x0020u, // C1_CNTRL
      0x0004u, // C1_DIGIT
      0x0004u, // C1_DIGIT
      (~(0x0020u | 0x0008u | 0x0040) & 0x01ffu) |
          0x0400u, // not C1_CNTRL or C1_SPACE or C1_BLANK
      w32_regex_traits_implementation<charT>::mask_horizontal,
      0x0002u,                       // C1_LOWER
      0x0002u,                       // C1_LOWER
      (~0x0020u & 0x01ffu) | 0x0400, // not C1_CNTRL
      0x0010u,                       // C1_PUNCT
      0x0008u,                       // C1_SPACE
      0x0008u,                       // C1_SPACE
      0x0001u,                       // C1_UPPER
      w32_regex_traits_implementation<charT>::mask_unicode,
      0x0001u, // C1_UPPER
      w32_regex_traits_implementation<charT>::mask_vertical,
      0x0104u | w32_regex_traits_implementation<charT>::mask_word,
      0x0104u | w32_regex_traits_implementation<charT>::mask_word,
      0x0080u, // C1_XDIGIT
  };
  std::size_t state_id =
      1u + (std::size_t)BOOST_REGEX_DETAIL_NS::get_default_class_id(p1, p2);
  if (state_id < sizeof(masks) / sizeof(masks[0]))
    return masks[state_id];
  return masks[0];
}

template <class charT>
std::shared_ptr<const w32_regex_traits_implementation<charT>>
create_w32_regex_traits(::boost::BOOST_REGEX_DETAIL_NS::lcid_type l) {
  // TODO: create a cache for previously constructed objects.
  return boost::object_cache<::boost::BOOST_REGEX_DETAIL_NS::lcid_type,
                             w32_regex_traits_implementation<charT>>::get(l, 5);
}

} // namespace BOOST_REGEX_DETAIL_NS

template <class charT> class w32_regex_traits {
public:
  typedef charT char_type;
  typedef std::size_t size_type;
  typedef std::basic_string<char_type> string_type;
  typedef ::boost::BOOST_REGEX_DETAIL_NS::lcid_type locale_type;
  typedef std::uint_least32_t char_class_type;

  w32_regex_traits()
      : m_pimpl(BOOST_REGEX_DETAIL_NS::create_w32_regex_traits<charT>(
            ::boost::BOOST_REGEX_DETAIL_NS::w32_get_default_locale())) {}
  static size_type length(const char_type *p) {
    return std::char_traits<charT>::length(p);
  }
  regex_constants::syntax_type syntax_type(charT c) const {
    return m_pimpl->syntax_type(c);
  }
  regex_constants::escape_syntax_type escape_syntax_type(charT c) const {
    return m_pimpl->escape_syntax_type(c);
  }
  charT translate(charT c) const { return c; }
  charT translate_nocase(charT c) const { return this->m_pimpl->tolower(c); }
  charT translate(charT c, bool icase) const {
    return icase ? this->m_pimpl->tolower(c) : c;
  }
  charT tolower(charT c) const { return this->m_pimpl->tolower(c); }
  charT toupper(charT c) const {
    return ::boost::BOOST_REGEX_DETAIL_NS::w32_toupper(c,
                                                       this->m_pimpl->m_locale);
  }
  string_type transform(const charT *p1, const charT *p2) const {
    return ::boost::BOOST_REGEX_DETAIL_NS::w32_transform(
        this->m_pimpl->m_locale, p1, p2);
  }
  string_type transform_primary(const charT *p1, const charT *p2) const {
    return m_pimpl->transform_primary(p1, p2);
  }
  char_class_type lookup_classname(const charT *p1, const charT *p2) const {
    return m_pimpl->lookup_classname(p1, p2);
  }
  string_type lookup_collatename(const charT *p1, const charT *p2) const {
    return m_pimpl->lookup_collatename(p1, p2);
  }
  bool isctype(charT c, char_class_type f) const {
    if ((f & BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                 charT>::mask_base) &&
        (this->m_pimpl->isctype(
            f & BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                    charT>::mask_base,
            c)))
      return true;
    else if ((f & BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                      charT>::mask_unicode) &&
             BOOST_REGEX_DETAIL_NS::is_extended(c))
      return true;
    else if ((f & BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                      charT>::mask_word) &&
             (c == '_'))
      return true;
    else if ((f & BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                      charT>::mask_vertical) &&
             (::boost::BOOST_REGEX_DETAIL_NS::is_separator(c) || (c == '\v')))
      return true;
    else if ((f & BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                      charT>::mask_horizontal) &&
             this->isctype(c, 0x0008u) &&
             !this->isctype(
                 c, BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<
                        charT>::mask_vertical))
      return true;
    return false;
  }
  std::intmax_t toi(const charT *&p1, const charT *p2, int radix) const {
    return ::boost::BOOST_REGEX_DETAIL_NS::global_toi(p1, p2, radix, *this);
  }
  int value(charT c, int radix) const {
    int result = (int)::boost::BOOST_REGEX_DETAIL_NS::global_value(c);
    return result < radix ? result : -1;
  }
  locale_type imbue(locale_type l) {
    ::boost::BOOST_REGEX_DETAIL_NS::lcid_type result(getloc());
    m_pimpl = BOOST_REGEX_DETAIL_NS::create_w32_regex_traits<charT>(l);
    return result;
  }
  locale_type getloc() const { return m_pimpl->m_locale; }
  std::string error_string(regex_constants::error_type n) const {
    return m_pimpl->error_string(n);
  }

private:
  std::shared_ptr<
      const BOOST_REGEX_DETAIL_NS::w32_regex_traits_implementation<charT>>
      m_pimpl;
};

namespace BOOST_REGEX_DETAIL_NS {

template <class U> inline void w32_regex_traits_char_layer<char>::init() {
  // we need to start by initialising our syntax map so we know which
  // character is used for which purpose:
  std::memset(m_char_map, 0, sizeof(m_char_map));
  for (regex_constants::syntax_type i = 1; i < regex_constants::syntax_max;
       ++i) {
    const char *ptr = get_default_syntax(i);
    while (ptr && *ptr) {
      m_char_map[static_cast<unsigned char>(*ptr)] = i;
      ++ptr;
    }
  }
  //
  // finish off by calculating our escape types:
  //
  unsigned char i = 'A';
  do {
    if (m_char_map[i] == 0) {
      if (::boost::BOOST_REGEX_DETAIL_NS::w32_is(this->m_locale, 0x0002u,
                                                 (char)i))
        m_char_map[i] = regex_constants::escape_type_class;
      else if (::boost::BOOST_REGEX_DETAIL_NS::w32_is(this->m_locale, 0x0001u,
                                                      (char)i))
        m_char_map[i] = regex_constants::escape_type_not_class;
    }
  } while (0xFF != i++);

  //
  // fill in lower case map:
  //
  char char_map[1 << CHAR_BIT];
  for (int ii = 0; ii < (1 << CHAR_BIT); ++ii)
    char_map[ii] = static_cast<char>(ii);
  int r = boost::BOOST_REGEX_DETAIL_NS::LCMapStringA(
      this->m_locale, lcmap_lowercase, char_map, 1 << CHAR_BIT,
      this->m_lower_map, 1 << CHAR_BIT);
  BOOST_REGEX_ASSERT(r != 0);
  if (r < (1 << CHAR_BIT)) {
    // if we have multibyte characters then not all may have been given
    // a lower case mapping:
    for (int jj = r; jj < (1 << CHAR_BIT); ++jj)
      this->m_lower_map[jj] = static_cast<char>(jj);
  }

  // Project patch: ANSI DBCS classification of the concatenated 256-byte
  // range writes fewer WORDs than bytes. Classify each byte independently,
  // as the native w32_is_* functions do, and initialize failed bytes to zero.
  for (int ii = 0; ii < (1 << CHAR_BIT); ++ii) {
    word mask = 0;
    const char byte = static_cast<char>(ii);
    this->m_type_map[ii] = boost::BOOST_REGEX_DETAIL_NS::GetStringTypeExA(
                               this->m_locale, ct_ctype1, &byte, 1, &mask)
                               ? mask
                               : 0;
  }
}

inline lcid_type w32_get_default_locale() {
  return boost::BOOST_REGEX_DETAIL_NS::GetUserDefaultLCID();
}

inline std::string w32_transform(lcid_type idx, const char *p1,
                                 const char *p2) {
  int bytes = boost::BOOST_REGEX_DETAIL_NS::LCMapStringA(
      idx,                       // locale identifier
      lcmap_sortkey,             // mapping transformation type
      p1,                        // source string
      static_cast<int>(p2 - p1), // number of characters in source string
      0,                         // destination buffer
      0                          // size of destination buffer
  );
  if (!bytes)
    return std::string(p1, p2);
  std::string result(++bytes, '\0');
  bytes = boost::BOOST_REGEX_DETAIL_NS::LCMapStringA(
      idx,                       // locale identifier
      lcmap_sortkey,             // mapping transformation type
      p1,                        // source string
      static_cast<int>(p2 - p1), // number of characters in source string
      &*result.begin(),          // destination buffer
      bytes                      // size of destination buffer
  );
  if (bytes > static_cast<int>(result.size()))
    return std::string(p1, p2);
  while (result.size() && result[result.size() - 1] == '\0') {
    result.erase(result.size() - 1);
  }
  return result;
}

inline char w32_toupper(char c, lcid_type idx) {
  char result[2];
  int b = boost::BOOST_REGEX_DETAIL_NS::LCMapStringA(
      idx,             // locale identifier
      lcmap_uppercase, // mapping transformation type
      &c,              // source string
      1,               // number of characters in source string
      result,          // destination buffer
      1);              // size of destination buffer
  if (b == 0)
    return c;
  return result[0];
}

inline bool w32_is(lcid_type idx, std::uint32_t m, char c) {
  word mask;
  if (boost::BOOST_REGEX_DETAIL_NS::GetStringTypeExA(idx, ct_ctype1, &c, 1,
                                                     &mask) &&
      (mask & m & w32_regex_traits_implementation<char>::mask_base))
    return true;
  if ((m & w32_regex_traits_implementation<char>::mask_word) && (c == '_'))
    return true;
  return false;
}

} // namespace BOOST_REGEX_DETAIL_NS

} // namespace boost

#ifdef BOOST_REGEX_MSVC
#pragma warning(pop)
#endif

#endif
