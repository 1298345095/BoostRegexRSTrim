/*
 *
 * Copyright (c) 1998-2009 John Maddock
 * Copyright 2008 Eric Niebler. 
 *
 * Use, modification and distribution are subject to the 
 * Boost Software License, Version 1.0. (See accompanying file 
 * LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org for most recent version.
  *   FILE         regex_format.hpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Provides formatting output routines for search and replace
  *                operations.  Note this is an internal header file included
  *                by regex.hpp, do not include on its own.
  */

#ifndef BOOST_REGEX_FORMAT_HPP
#define BOOST_REGEX_FORMAT_HPP

#include <boost/regex/v5/match_flags.hpp>
#include <boost/regex/v5/sub_match.hpp>
#include <boost/regex/v5/regex_traits_defaults.hpp>
#include <vector>

namespace boost {

//
// Forward declaration:
//
template <class BidiIterator,
          class Allocator =
              typename std::vector<sub_match<BidiIterator>>::allocator_type>
class match_results;

namespace BOOST_REGEX_DETAIL_NS {

#ifdef BOOST_REGEX_MSVC
#pragma warning(push)
#pragma warning(disable : 26812)
#endif
template <class OutputIterator, class Results, class traits>
class basic_regex_formatter {
public:
  typedef typename traits::char_type char_type;
  basic_regex_formatter(OutputIterator o, const Results &r, const traits &t)
      : m_traits(t), m_results(r), m_out(o), m_position(), m_end(),
        m_state(output_copy), m_restore_state(output_copy) {}
  OutputIterator format(const char_type *p1, const char_type *p2);

private:
  typedef typename Results::value_type sub_match_type;
  enum output_state {
    output_copy,
    output_next_lower,
    output_next_upper,
    output_lower,
    output_upper
  };

  void put(char_type c);
  void put(const sub_match_type &sub);
  void format_all();
  void format_perl();
  void format_escape();
  bool handle_perl_verb(bool have_brace);

  inline typename Results::value_type const &get_named_sub(const char_type *i,
                                                           const char_type *j) {
    return this->m_results.named_subexpression(i, j);
  }
  inline int toi(const char_type *&i, const char_type *j, int base) {
    return i == j ? -1 : static_cast<int>(m_traits.toi(i, j, base));
  }

  const traits
      &m_traits; // the traits class for localised formatting operations
  const Results &m_results;     // the match_results being used.
  OutputIterator m_out;         // where to send output.
  const char_type *m_position;  // format string, current position
  const char_type *m_end;       // format string end
  output_state m_state;         // what to do with the next character
  output_state m_restore_state; // what state to restore to.
private:
  basic_regex_formatter(const basic_regex_formatter &);
  basic_regex_formatter &operator=(const basic_regex_formatter &);
};
#ifdef BOOST_REGEX_MSVC
#pragma warning(pop)
#endif

template <class OutputIterator, class Results, class traits>
OutputIterator basic_regex_formatter<OutputIterator, Results, traits>::format(
    const char_type *p1, const char_type *p2) {
  m_position = p1;
  m_end = p2;
  format_all();
  return m_out;
}

template <class OutputIterator, class Results, class traits>
void basic_regex_formatter<OutputIterator, Results, traits>::format_all() {
  // over and over:
  while (m_position != m_end) {
    switch (*m_position) {
    case '\\':
      format_escape();
      break;
    case '$':
      format_perl();
      break;
    default:
      put(*m_position);
      ++m_position;
      break;
    }
  }
}

template <class OutputIterator, class Results, class traits>
void basic_regex_formatter<OutputIterator, Results, traits>::format_perl() {
  //
  // On entry *m_position points to a '$' character
  // output the information that goes with it:
  //
  BOOST_REGEX_ASSERT(*m_position == '$');
  //
  // see if this is a trailing '$':
  //
  if (++m_position == m_end) {
    --m_position;
    put(*m_position);
    ++m_position;
    return;
  }
  //
  // OK find out what kind it is:
  //
  bool have_brace = false;
  const char_type *save_position = m_position;
  switch (*m_position) {
  case '&':
    ++m_position;
    put(this->m_results[0]);
    break;
  case '`':
    ++m_position;
    put(this->m_results.prefix());
    break;
  case '\'':
    ++m_position;
    put(this->m_results.suffix());
    break;
  case '$':
    put(*m_position++);
    break;
  case '+':
    if ((++m_position != m_end) && (*m_position == '{')) {
      const char_type *base = ++m_position;
      while ((m_position != m_end) && (*m_position != '}'))
        ++m_position;
      if (m_position != m_end) {
        // Named sub-expression:
        put(get_named_sub(base, m_position));
        ++m_position;
        break;
      } else {
        m_position = --base;
      }
    }
    put((this->m_results)[this->m_results.size() > 1
                              ? static_cast<int>(this->m_results.size() - 1)
                              : 1]);
    break;
  case '{':
    have_brace = true;
    ++m_position;
    [[fallthrough]];
  default:
    // see if we have a number:
    {
      std::ptrdiff_t len = std::distance(m_position, m_end);
      //len = (std::min)(static_cast<std::ptrdiff_t>(2), len);
      int v = this->toi(m_position, m_position + len, 10);
      if ((v < 0) ||
          (have_brace && ((m_position == m_end) || (*m_position != '}')))) {
        // Look for a Perl-5.10 verb:
        if (!handle_perl_verb(have_brace)) {
          // leave the $ as is, and carry on:
          m_position = --save_position;
          put(*m_position);
          ++m_position;
        }
        break;
      }
      // otherwise output sub v:
      put(this->m_results[v]);
      if (have_brace)
        ++m_position;
    }
  }
}

template <class OutputIterator, class Results, class traits>
bool basic_regex_formatter<OutputIterator, Results, traits>::handle_perl_verb(
    bool have_brace) {
  //
  // We may have a capitalised string containing a Perl action:
  //
  static const char_type MATCH[] = {'M', 'A', 'T', 'C', 'H'};
  static const char_type PREMATCH[] = {'P', 'R', 'E', 'M', 'A', 'T', 'C', 'H'};
  static const char_type POSTMATCH[] = {'P', 'O', 'S', 'T', 'M',
                                        'A', 'T', 'C', 'H'};
  static const char_type LAST_PAREN_MATCH[] = {'L', 'A', 'S', 'T', '_', 'P',
                                               'A', 'R', 'E', 'N', '_', 'M',
                                               'A', 'T', 'C', 'H'};
  static const char_type LAST_SUBMATCH_RESULT[] = {
      'L', 'A', 'S', 'T', '_', 'S', 'U', 'B', 'M', 'A',
      'T', 'C', 'H', '_', 'R', 'E', 'S', 'U', 'L', 'T'};
  static const char_type LAST_SUBMATCH_RESULT_ALT[] = {'^', 'N'};

  if (m_position == m_end)
    return false;
  if (have_brace && (*m_position == '^'))
    ++m_position;

  std::ptrdiff_t max_len = m_end - m_position;

  if ((max_len >= 5) && std::equal(m_position, m_position + 5, MATCH)) {
    m_position += 5;
    if (have_brace) {
      if ((m_position != m_end) && (*m_position == '}'))
        ++m_position;
      else {
        m_position -= 5;
        return false;
      }
    }
    put(this->m_results[0]);
    return true;
  }
  if ((max_len >= 8) && std::equal(m_position, m_position + 8, PREMATCH)) {
    m_position += 8;
    if (have_brace) {
      if ((m_position != m_end) && (*m_position == '}'))
        ++m_position;
      else {
        m_position -= 8;
        return false;
      }
    }
    put(this->m_results.prefix());
    return true;
  }
  if ((max_len >= 9) && std::equal(m_position, m_position + 9, POSTMATCH)) {
    m_position += 9;
    if (have_brace) {
      if ((m_position != m_end) && (*m_position == '}'))
        ++m_position;
      else {
        m_position -= 9;
        return false;
      }
    }
    put(this->m_results.suffix());
    return true;
  }
  if ((max_len >= 16) &&
      std::equal(m_position, m_position + 16, LAST_PAREN_MATCH)) {
    m_position += 16;
    if (have_brace) {
      if ((m_position != m_end) && (*m_position == '}'))
        ++m_position;
      else {
        m_position -= 16;
        return false;
      }
    }
    put((this->m_results)[this->m_results.size() > 1
                              ? static_cast<int>(this->m_results.size() - 1)
                              : 1]);
    return true;
  }
  if ((max_len >= 20) &&
      std::equal(m_position, m_position + 20, LAST_SUBMATCH_RESULT)) {
    m_position += 20;
    if (have_brace) {
      if ((m_position != m_end) && (*m_position == '}'))
        ++m_position;
      else {
        m_position -= 20;
        return false;
      }
    }
    put(this->m_results.get_last_closed_paren());
    return true;
  }
  if ((max_len >= 2) &&
      std::equal(m_position, m_position + 2, LAST_SUBMATCH_RESULT_ALT)) {
    m_position += 2;
    if (have_brace) {
      if ((m_position != m_end) && (*m_position == '}'))
        ++m_position;
      else {
        m_position -= 2;
        return false;
      }
    }
    put(this->m_results.get_last_closed_paren());
    return true;
  }
  return false;
}

template <class OutputIterator, class Results, class traits>
void basic_regex_formatter<OutputIterator, Results, traits>::format_escape() {
  // skip the escape and check for trailing escape:
  if (++m_position == m_end) {
    put(static_cast<char_type>('\\'));
    return;
  }
  // now switch on the escape type:
  switch (*m_position) {
  case 'a':
    put(static_cast<char_type>('\a'));
    ++m_position;
    break;
  case 'f':
    put(static_cast<char_type>('\f'));
    ++m_position;
    break;
  case 'n':
    put(static_cast<char_type>('\n'));
    ++m_position;
    break;
  case 'r':
    put(static_cast<char_type>('\r'));
    ++m_position;
    break;
  case 't':
    put(static_cast<char_type>('\t'));
    ++m_position;
    break;
  case 'v':
    put(static_cast<char_type>('\v'));
    ++m_position;
    break;
  case 'x':
    if (++m_position == m_end) {
      put(static_cast<char_type>('x'));
      return;
    }
    // maybe have \x{ddd}
    if (*m_position == static_cast<char_type>('{')) {
      ++m_position;
      int val = this->toi(m_position, m_end, 16);
      if (val < 0) {
        // invalid value treat everything as literals:
        put(static_cast<char_type>('x'));
        put(static_cast<char_type>('{'));
        return;
      }
      if ((m_position == m_end) ||
          (*m_position != static_cast<char_type>('}'))) {
        --m_position;
        while (*m_position != static_cast<char_type>('\\'))
          --m_position;
        ++m_position;
        put(*m_position++);
        return;
      }
      ++m_position;
      put(static_cast<char_type>(val));
      return;
    } else {
      std::ptrdiff_t len = std::distance(m_position, m_end);
      len = (std::min)(static_cast<std::ptrdiff_t>(2), len);
      int val = this->toi(m_position, m_position + len, 16);
      if (val < 0) {
        --m_position;
        put(*m_position++);
        return;
      }
      put(static_cast<char_type>(val));
    }
    break;
  case 'c':
    if (++m_position == m_end) {
      --m_position;
      put(*m_position++);
      return;
    }
    put(static_cast<char_type>(*m_position++ % 32));
    break;
  case 'e':
    put(static_cast<char_type>(27));
    ++m_position;
    break;
  default:
    // see if we have a perl specific escape:
    bool breakout = false;
    switch (*m_position) {
    case 'l':
      ++m_position;
      m_restore_state = m_state;
      m_state = output_next_lower;
      breakout = true;
      break;
    case 'L':
      ++m_position;
      m_state = output_lower;
      breakout = true;
      break;
    case 'u':
      ++m_position;
      m_restore_state = m_state;
      m_state = output_next_upper;
      breakout = true;
      break;
    case 'U':
      ++m_position;
      m_state = output_upper;
      breakout = true;
      break;
    case 'E':
      ++m_position;
      m_state = output_copy;
      breakout = true;
      break;
    }
    if (breakout)
      break;
    // see if we have a \n sed style backreference:
    std::ptrdiff_t len = std::distance(m_position, m_end);
    len = (std::min)(static_cast<std::ptrdiff_t>(1), len);
    int v = this->toi(m_position, m_position + len, 10);
    if (v > 0) {
      put(m_results[v]);
      break;
    } else if (v == 0) {
      // octal ecape sequence:
      --m_position;
      len = std::distance(m_position, m_end);
      len = (std::min)(static_cast<std::ptrdiff_t>(4), len);
      v = this->toi(m_position, m_position + len, 8);
      BOOST_REGEX_ASSERT(v >= 0);
      put(static_cast<char_type>(v));
      break;
    }
    // Otherwise output the character "as is":
    put(*m_position++);
    break;
  }
}

template <class OutputIterator, class Results, class traits>
void basic_regex_formatter<OutputIterator, Results, traits>::put(char_type c) {
  // write a single character to output
  // according to which case translation mode we are in:
  switch (this->m_state) {
  case output_next_lower:
    c = m_traits.tolower(c);
    this->m_state = m_restore_state;
    break;
  case output_next_upper:
    c = m_traits.toupper(c);
    this->m_state = m_restore_state;
    break;
  case output_lower:
    c = m_traits.tolower(c);
    break;
  case output_upper:
    c = m_traits.toupper(c);
    break;
  default:
    break;
  }
  *m_out = c;
  ++m_out;
}

template <class OutputIterator, class Results, class traits>
void basic_regex_formatter<OutputIterator, Results, traits>::put(
    const sub_match_type &sub) {
  typedef typename sub_match_type::iterator iterator_type;
  iterator_type i = sub.first;
  while (i != sub.second) {
    put(*i);
    ++i;
  }
}

template <class OutputIterator, class Iterator, class Alloc, class traits>
OutputIterator
regex_format_imp(OutputIterator out, const match_results<Iterator, Alloc> &m,
                 const typename traits::char_type *p1,
                 const typename traits::char_type *p2, const traits &t) {
  BOOST_REGEX_DETAIL_NS::basic_regex_formatter<
      OutputIterator, match_results<Iterator, Alloc>, traits>
      f(out, m, t);
  return f.format(p1, p2);
}

} // namespace BOOST_REGEX_DETAIL_NS
} // namespace boost

#endif // BOOST_REGEX_FORMAT_HPP
