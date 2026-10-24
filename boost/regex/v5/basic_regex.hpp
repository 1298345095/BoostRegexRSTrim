/*
 *
 * Copyright (c) 1998-2004 John Maddock
 * Copyright 2011 Garmin Ltd. or its subsidiaries
 *
 * Distributed under the Boost Software License, Version 1.0.
 * (See accompanying file LICENSE_1_0.txt or copy at
 * http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org/ for most recent version.
  *   FILE         basic_regex.cpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Declares template class basic_regex.
  */

#ifndef BOOST_REGEX_V5_BASIC_REGEX_HPP
#define BOOST_REGEX_V5_BASIC_REGEX_HPP

#include <boost/regex/v5/regbase.hpp>
#include <boost/regex/v5/syntax_type.hpp>
#include <boost/regex/regex_traits.hpp>
#include <boost/regex/v5/states.hpp>
#include <boost/regex/v5/regex_raw_buffer.hpp>

#include <algorithm>
#include <limits>
#include <memory>
#include <vector>

namespace boost {
#ifdef BOOST_REGEX_MSVC
#pragma warning(push)
#pragma warning(disable : 4251)
#if BOOST_REGEX_MSVC < 1700
#pragma warning(disable : 4231)
#endif
#if BOOST_REGEX_MSVC < 1600
#pragma warning(disable : 4660)
#endif
#if BOOST_REGEX_MSVC < 1910
#pragma warning(disable : 4800)
#endif
#endif

namespace BOOST_REGEX_DETAIL_NS {

//
// forward declaration, we will need this one later:
//
template <class charT, class traits> class basic_regex_parser;

template <class I> void bubble_down_one(I first, I last) {
  if (first != last) {
    I next = last - 1;
    while ((next != first) && (*next < *(next - 1))) {
      (next - 1)->swap(*next);
      --next;
    }
  }
}

static const int hash_value_mask = 1 << (std::numeric_limits<int>::digits - 1);

template <class Iterator>
inline int hash_value_from_capture_name(Iterator i, Iterator j) {
  std::size_t r = 0;
  while (i != j) {
    r ^= *i + 0x9e3779b9 + (r << 6) + (r >> 2);
    ++i;
  }
  r %= ((std::numeric_limits<int>::max)());
  return static_cast<int>(r) | hash_value_mask;
}

class named_subexpressions {
public:
  struct name {
    template <class charT>
    name(const charT *i, const charT *j, int idx) : index(idx) {
      hash = hash_value_from_capture_name(i, j);
    }
    name(int h, int idx) : index(idx), hash(h) {}
    int index;
    int hash;
    bool operator<(const name &other) const { return hash < other.hash; }
    bool operator==(const name &other) const { return hash == other.hash; }
    void swap(name &other) {
      std::swap(index, other.index);
      std::swap(hash, other.hash);
    }
  };

  typedef std::vector<name>::const_iterator const_iterator;
  typedef std::pair<const_iterator, const_iterator> range_type;

  named_subexpressions() {}

  template <class charT>
  void set_name(const charT *i, const charT *j, int index) {
    m_sub_names.push_back(name(i, j, index));
    bubble_down_one(m_sub_names.begin(), m_sub_names.end());
  }
  template <class charT> int get_id(const charT *i, const charT *j) const {
    name t(i, j, 0);
    typename std::vector<name>::const_iterator pos =
        std::lower_bound(m_sub_names.begin(), m_sub_names.end(), t);
    if ((pos != m_sub_names.end()) && (*pos == t)) {
      return pos->index;
    }
    return -1;
  }
  template <class charT>
  range_type equal_range(const charT *i, const charT *j) const {
    name t(i, j, 0);
    return std::equal_range(m_sub_names.begin(), m_sub_names.end(), t);
  }
  int get_id(int h) const {
    name t(h, 0);
    std::vector<name>::const_iterator pos =
        std::lower_bound(m_sub_names.begin(), m_sub_names.end(), t);
    if ((pos != m_sub_names.end()) && (*pos == t)) {
      return pos->index;
    }
    return -1;
  }
  range_type equal_range(int h) const {
    name t(h, 0);
    return std::equal_range(m_sub_names.begin(), m_sub_names.end(), t);
  }

private:
  std::vector<name> m_sub_names;
};

//
// class regex_data:
// represents the data we wish to expose to the matching algorithms.
//
template <class charT, class traits>
struct regex_data : public named_subexpressions {
  typedef regex_constants::syntax_option_type flag_type;
  typedef std::size_t size_type;

  regex_data(const ::std::shared_ptr<::boost::regex_traits_wrapper<traits>> &t)
      : m_ptraits(t), m_flags(0), m_status(0), m_expression(0),
        m_expression_len(0), m_mark_count(0), m_first_state(0),
        m_restart_type(0), m_startmap{0}, m_can_be_null(0), m_word_mask(0),
        m_has_recursions(false), m_disable_match_any(false) {}
  regex_data()
      : m_ptraits(new ::boost::regex_traits_wrapper<traits>()), m_flags(0),
        m_status(0), m_expression(0), m_expression_len(0), m_mark_count(0),
        m_first_state(0), m_restart_type(0), m_startmap{0}, m_can_be_null(0),
        m_word_mask(0), m_has_recursions(false), m_disable_match_any(false) {}

  ::std::shared_ptr<::boost::regex_traits_wrapper<traits>>
      m_ptraits;                   // traits class instance
  flag_type m_flags;               // flags with which we were compiled
  int m_status;                    // error code (0 implies OK).
  const charT *m_expression;       // the original expression
  std::ptrdiff_t m_expression_len; // the length of the original expression
  size_type m_mark_count;          // the number of marked sub-expressions
  BOOST_REGEX_DETAIL_NS::re_syntax_base
      *m_first_state;                      // the first state of the machine
  unsigned m_restart_type;                 // search optimisation type
  unsigned char m_startmap[1 << CHAR_BIT]; // which characters can start a match
  unsigned int m_can_be_null;              // whether we can match a null string
  BOOST_REGEX_DETAIL_NS::raw_storage
      m_data; // the buffer in which our states are constructed
  typename traits::char_class_type
      m_word_mask; // mask used to determine if a character is a word character
  bool m_has_recursions; // whether we have recursive expressions;
  bool
      m_disable_match_any; // when set we need to disable the match_any flag as it causes different/buggy behaviour.
};
//
// class basic_regex_implementation
// pimpl implementation class for basic_regex.
//
template <class charT, class traits>
class basic_regex_implementation : public regex_data<charT, traits> {
public:
  typedef regex_constants::syntax_option_type flag_type;
  typedef std::ptrdiff_t difference_type;
  typedef std::size_t size_type;
  typedef typename traits::locale_type locale_type;
  typedef const charT *const_iterator;

  basic_regex_implementation() {}
  basic_regex_implementation(
      const ::std::shared_ptr<::boost::regex_traits_wrapper<traits>> &t)
      : regex_data<charT, traits>(t) {}
  void assign(const charT *arg_first, const charT *arg_last, flag_type f) {
    regex_data<charT, traits> *pdat = this;
    basic_regex_parser<charT, traits> parser(pdat);
    parser.parse(arg_first, arg_last, f);
  }

  flag_type flags() const { return this->m_flags; }
  size_type size() const { return this->m_expression_len; }
  int status() const { return this->m_status; }
  size_type mark_count() const { return this->m_mark_count - 1; }
  const BOOST_REGEX_DETAIL_NS::re_syntax_base *get_first_state() const {
    return this->m_first_state;
  }
  unsigned get_restart_type() const { return this->m_restart_type; }
  const unsigned char *get_map() const { return this->m_startmap; }
  const ::boost::regex_traits_wrapper<traits> &get_traits() const {
    return *(this->m_ptraits);
  }
  bool can_be_null() const { return this->m_can_be_null; }
  const regex_data<charT, traits> &get_data() const {
    basic_regex_implementation<charT, traits> const *p = this;
    return *static_cast<const regex_data<charT, traits> *>(p);
  }
};

} // namespace BOOST_REGEX_DETAIL_NS
//
// class basic_regex:
// represents the compiled
// regular expression:
//

template <class charT, class traits> class basic_regex : public regbase {
public:
  // typedefs:
  typedef std::size_t traits_size_type;
  typedef typename traits::string_type traits_string_type;
  typedef charT char_type;
  typedef traits traits_type;

  typedef charT value_type;
  typedef charT &reference;
  typedef const charT &const_reference;
  typedef const charT *const_iterator;
  typedef const_iterator iterator;
  typedef std::ptrdiff_t difference_type;
  typedef std::size_t size_type;
  typedef regex_constants::syntax_option_type flag_type;
  // locale_type
  // placeholder for actual locale type used by the
  // traits class to localise *this.
  typedef typename traits::locale_type locale_type;

public:
  explicit basic_regex() {}
  basic_regex(const basic_regex &that) : m_pimpl(that.m_pimpl) {}
  ~basic_regex() {}
  basic_regex &operator=(const basic_regex &that) { return assign(that); }
  //
  // assign:
  basic_regex &assign(const basic_regex &that) {
    m_pimpl = that.m_pimpl;
    return *this;
  }

private:
  basic_regex &do_assign(const charT *p1, const charT *p2, flag_type f);

public:
  basic_regex &assign(const charT *p1, const charT *p2,
                      flag_type f = regex_constants::normal) {
    return do_assign(p1, p2, f);
  }

  flag_type flags() const { return m_pimpl.get() ? m_pimpl->flags() : 0; }
  //
  // size:
  size_type size() const { return (m_pimpl.get() ? m_pimpl->size() : 0); }
  //
  // empty:
  bool empty() const { return (m_pimpl.get() ? 0 != m_pimpl->status() : true); }

  size_type mark_count() const {
    return (m_pimpl.get() ? m_pimpl->mark_count() : 0);
  }

  //
  // private access methods:
  //
  const BOOST_REGEX_DETAIL_NS::re_syntax_base *get_first_state() const {
    BOOST_REGEX_ASSERT(0 != m_pimpl.get());
    return m_pimpl->get_first_state();
  }
  unsigned get_restart_type() const {
    BOOST_REGEX_ASSERT(0 != m_pimpl.get());
    return m_pimpl->get_restart_type();
  }
  const unsigned char *get_map() const {
    BOOST_REGEX_ASSERT(0 != m_pimpl.get());
    return m_pimpl->get_map();
  }
  const ::boost::regex_traits_wrapper<traits> &get_traits() const {
    BOOST_REGEX_ASSERT(0 != m_pimpl.get());
    return m_pimpl->get_traits();
  }
  bool can_be_null() const {
    BOOST_REGEX_ASSERT(0 != m_pimpl.get());
    return m_pimpl->can_be_null();
  }
  const BOOST_REGEX_DETAIL_NS::regex_data<charT, traits> &get_data() const {
    BOOST_REGEX_ASSERT(0 != m_pimpl.get());
    return m_pimpl->get_data();
  }
  std::shared_ptr<BOOST_REGEX_DETAIL_NS::named_subexpressions>
  get_named_subs() const {
    return m_pimpl;
  }

private:
  std::shared_ptr<
      BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, traits>>
      m_pimpl;
};

//
// out of line members;
// these are the only members that mutate the basic_regex object,
// and are designed to provide the strong exception guarantee
// (in the event of a throw, the state of the object remains unchanged).
//

namespace detail {
template <class charT, class F, class Traits>
std::shared_ptr<
    BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, Traits>>
create_implemenation(
    const charT *p1, const charT *p2, F f,
    std::shared_ptr<boost::regex_traits_wrapper<Traits>> ptraits) {
  std::shared_ptr<
      BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, Traits>>
      result;
  if (!ptraits.get()) {
    result = std::shared_ptr<
        BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, Traits>>(
        new BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, Traits>());
  } else {
    result = std::shared_ptr<
        BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, Traits>>(
        new BOOST_REGEX_DETAIL_NS::basic_regex_implementation<charT, Traits>(
            ptraits));
  }
  result->assign(p1, p2, f);
  return result;
}
} // namespace detail

template <class charT, class traits>
basic_regex<charT, traits> &
basic_regex<charT, traits>::do_assign(const charT *p1, const charT *p2,
                                      flag_type f) {
  m_pimpl = detail::create_implemenation(
      p1, p2, f,
      m_pimpl.get() ? m_pimpl->m_ptraits
                    : std::shared_ptr<boost::regex_traits_wrapper<traits>>());
  return *this;
}

#ifdef BOOST_REGEX_MSVC
#pragma warning(pop)
#endif

} // namespace boost

#endif
