/*
 *
 * Copyright (c) 1998-2009
 * John Maddock
 *
 * Use, modification and distribution are subject to the 
 * Boost Software License, Version 1.0. (See accompanying file 
 * LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org for most recent version.
  *   FILE         match_results.cpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Declares template class match_results.
  */

#ifndef BOOST_REGEX_V5_MATCH_RESULTS_HPP
#define BOOST_REGEX_V5_MATCH_RESULTS_HPP

#include <boost/regex/v5/match_flags.hpp>
#include <boost/regex/v5/sub_match.hpp>
#include <boost/regex/v5/basic_regex.hpp>
#include <boost/regex/v5/regex_format.hpp>

#include <string>
#include <type_traits>
#include <vector>

namespace boost {
#ifdef BOOST_REGEX_MSVC
#pragma warning(push)
#pragma warning(disable : 4251 4459)
#if BOOST_REGEX_MSVC < 1700
#pragma warning(disable : 4231)
#endif
#if BOOST_REGEX_MSVC < 1600
#pragma warning(disable : 4660)
#endif
#endif

namespace BOOST_REGEX_DETAIL_NS {

class named_subexpressions;

}

template <class BidiIterator, class Allocator> class match_results {
private:
  typedef std::vector<sub_match<BidiIterator>, Allocator> vector_type;

public:
  typedef sub_match<BidiIterator> value_type;
  typedef typename std::allocator_traits<Allocator>::value_type const
      &const_reference;
  typedef const_reference reference;
  typedef typename vector_type::const_iterator const_iterator;
  typedef const_iterator iterator;
  typedef typename std::iterator_traits<BidiIterator>::difference_type
      difference_type;
  typedef typename std::allocator_traits<Allocator>::size_type size_type;
  typedef Allocator allocator_type;
  typedef typename std::iterator_traits<BidiIterator>::value_type char_type;
  typedef std::basic_string<char_type> string_type;
  typedef BOOST_REGEX_DETAIL_NS::named_subexpressions named_sub_type;

  // construct/copy/destroy:
  explicit match_results(const Allocator &a = Allocator())
      : m_subs(a), m_base(), m_null(), m_last_closed_paren(0),
        m_is_singular(true) {}
  //
  // IMPORTANT: in the code below, the crazy looking checks around m_is_singular are
  // all required because it is illegal to copy a singular iterator.
  // See https://svn.boost.org/trac/boost/ticket/3632.
  //
  match_results(const match_results &m)
      : m_subs(m.m_subs), m_base(), m_null(), m_named_subs(m.m_named_subs),
        m_last_closed_paren(m.m_last_closed_paren),
        m_is_singular(m.m_is_singular) {
    if (!m_is_singular) {
      m_base = m.m_base;
      m_null = m.m_null;
    }
  }
  match_results &operator=(const match_results &m) {
    m_subs = m.m_subs;
    m_named_subs = m.m_named_subs;
    m_last_closed_paren = m.m_last_closed_paren;
    m_is_singular = m.m_is_singular;
    if (!m_is_singular) {
      m_base = m.m_base;
      m_null = m.m_null;
    }
    return *this;
  }
  ~match_results() {}

  // size:
  size_type size() const { return empty() ? 0 : m_subs.size() - 2; }
  size_type max_size() const { return m_subs.max_size(); }
  bool empty() const { return m_subs.size() < 2; }
  // element access:
private:
  difference_type do_get_length(int sub = 0) const {
    if (m_is_singular)
      raise_logic_error();
    sub += 2;
    if ((sub < (int)m_subs.size()) && (sub > 0))
      return m_subs[sub].length();
    return 0;
  }

public:
  difference_type length() const { return do_get_length(0); }
  difference_type position(size_type sub = 0) const {
    if (m_is_singular)
      raise_logic_error();
    sub += 2;
    if (sub < m_subs.size()) {
      const sub_match<BidiIterator> &s = m_subs[sub];
      if (s.matched || (sub == 2)) {
        return std::distance((BidiIterator)(m_base), (BidiIterator)(s.first));
      }
    }
    return ~static_cast<difference_type>(0);
  }

private:
  const_reference get_at(int sub) const {
    if (m_is_singular && m_subs.empty())
      raise_logic_error();

    if (sub >= INT_MAX - 2)
      return m_null;

    sub += 2;
    if (sub < (int)m_subs.size() && (sub >= 0)) {
      return m_subs[sub];
    }
    return m_null;
  }

public:
  template <class Integer>
  typename std::enable_if<std::is_integral<Integer>::value,
                          const_reference>::type
  operator[](Integer sub) const {
    return get_at(static_cast<int>(sub));
  }
  //
  // Named sub-expressions:
  //
  const_reference named_subexpression(const char_type *i,
                                      const char_type *j) const {
    //
    // Scan for the leftmost *matched* subexpression with the specified named:
    //
    if (m_is_singular)
      raise_logic_error();
    BOOST_REGEX_DETAIL_NS::named_subexpressions::range_type r =
        m_named_subs->equal_range(i, j);
    while ((r.first != r.second) && ((*this)[r.first->index].matched == false))
      ++r.first;
    return r.first != r.second ? (*this)[r.first->index] : m_null;
  }
  const_reference prefix() const {
    if (m_is_singular)
      raise_logic_error();
    return (*this)[-1];
  }

  const_reference suffix() const {
    if (m_is_singular)
      raise_logic_error();
    return (*this)[-2];
  }
  // The bridge supplies a replacement string and the compiled regex traits.
  template <class OutputIterator, class RegexT>
  OutputIterator format(OutputIterator out, const string_type &fmt,
                        const RegexT &re) const {
    if (m_is_singular)
      raise_logic_error();
    return BOOST_REGEX_DETAIL_NS::regex_format_imp(
        out, *this, fmt.data(), fmt.data() + fmt.size(), re.get_traits());
  }

  const_reference get_last_closed_paren() const {
    if (m_is_singular)
      raise_logic_error();
    return m_last_closed_paren == 0 ? m_null : (*this)[m_last_closed_paren];
  }

  //
  // private access functions:
  void set_second(BidiIterator i) {
    BOOST_REGEX_ASSERT(m_subs.size() > 2);
    m_subs[2].second = i;
    m_subs[2].matched = true;
    m_subs[0].first = i;
    m_subs[0].matched = (m_subs[0].first != m_subs[0].second);
    m_null.first = i;
    m_null.second = i;
    m_null.matched = false;
    m_is_singular = false;
  }

  void set_second(BidiIterator i, size_type pos, bool m = true,
                  bool escape_k = false) {
    if (pos)
      m_last_closed_paren = static_cast<int>(pos);
    pos += 2;
    BOOST_REGEX_ASSERT(m_subs.size() > pos);
    m_subs[pos].second = i;
    m_subs[pos].matched = m;
    if ((pos == 2) && !escape_k) {
      m_subs[0].first = i;
      m_subs[0].matched = (m_subs[0].first != m_subs[0].second);
      m_null.first = i;
      m_null.second = i;
      m_null.matched = false;
      m_is_singular = false;
    }
  }
  void set_size(size_type n, BidiIterator i, BidiIterator j) {
    value_type v(j);
    size_type len = m_subs.size();
    if (len > n + 2) {
      m_subs.erase(m_subs.begin() + n + 2, m_subs.end());
      std::fill(m_subs.begin(), m_subs.end(), v);
    } else {
      std::fill(m_subs.begin(), m_subs.end(), v);
      if (n + 2 != len)
        m_subs.insert(m_subs.end(), n + 2 - len, v);
    }
    m_subs[1].first = i;
    m_last_closed_paren = 0;
  }
  void set_base(BidiIterator pos) { m_base = pos; }
  BidiIterator base() const { return m_base; }
  void set_first(BidiIterator i) {
    BOOST_REGEX_ASSERT(m_subs.size() > 2);
    // set up prefix:
    m_subs[1].second = i;
    m_subs[1].matched = (m_subs[1].first != i);
    // set up $0:
    m_subs[2].first = i;
    // zero out everything else:
    for (size_type n = 3; n < m_subs.size(); ++n) {
      m_subs[n].first = m_subs[n].second = m_subs[0].second;
      m_subs[n].matched = false;
    }
  }
  void set_first(BidiIterator i, size_type pos, bool escape_k = false) {
    BOOST_REGEX_ASSERT(pos + 2 < m_subs.size());
    if (pos || escape_k) {
      m_subs[pos + 2].first = i;
      if (escape_k) {
        m_subs[1].second = i;
        m_subs[1].matched = (m_subs[1].first != m_subs[1].second);
      }
    } else
      set_first(i);
  }

  void set_named_subs(std::shared_ptr<named_sub_type> subs) {
    m_named_subs = std::move(subs);
  }

private:
  //
  // Error handler called when an uninitialized match_results is accessed:
  //
  static void raise_logic_error() {
    std::logic_error e(
        "Attempt to access an uninitialized boost::match_results<> class.");
    throw e;
  }

  vector_type m_subs;             // subexpressions
  BidiIterator m_base;            // where the search started from
  sub_match<BidiIterator> m_null; // a null match
  std::shared_ptr<named_sub_type>
      m_named_subs;        // Shared copy of named subs in the regex object
  int m_last_closed_paren; // Last ) to be seen - used for formatting
  bool m_is_singular;      // True if our stored iterators are singular
};

#ifdef BOOST_REGEX_MSVC
#pragma warning(pop)
#endif
} // namespace boost

#endif
