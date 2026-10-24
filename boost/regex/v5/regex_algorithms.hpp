/*
 * Copyright (c) 1998-2009 John Maddock
 * Distributed under the Boost Software License, Version 1.0.
 * See LICENSE_1_0.txt or http://www.boost.org/LICENSE_1_0.txt.
 */

// Matching, searching, iteration and replacement for the byte regex bridge.
// Kept in dependency order: search precedes iteration, which precedes replacement.
#ifndef BOOST_REGEX_V5_REGEX_ALGORITHMS_HPP
#define BOOST_REGEX_V5_REGEX_ALGORITHMS_HPP

#include <boost/regex/v5/basic_regex.hpp>
#include <boost/regex/v5/match_flags.hpp>
#include <boost/regex/v5/match_results.hpp>
#include <boost/regex/v5/perl_matcher.hpp>
#include <algorithm>
#include <memory>

// regex_match
namespace boost {

//
// proc regex_match
// returns true if the specified regular expression matches
// the whole of the input.  Fills in what matched in m.
//
template <class BidiIterator, class Allocator, class charT, class traits>
bool regex_match(BidiIterator first, BidiIterator last,
                 match_results<BidiIterator, Allocator> &m,
                 const basic_regex<charT, traits> &e,
                 match_flag_type flags = match_default) {
  BOOST_REGEX_DETAIL_NS::perl_matcher<BidiIterator, Allocator, traits> matcher(
      first, last, m, e, flags, first);
  return BOOST_REGEX_DETAIL_NS::factory_match(matcher);
}
template <class iterator, class charT, class traits>
bool regex_match(iterator first, iterator last,
                 const basic_regex<charT, traits> &e,
                 match_flag_type flags = match_default) {
  match_results<iterator> m;
  return regex_match(first, last, m, e, flags | regex_constants::match_any);
}
} // namespace boost

// regex_search
namespace boost {

template <class BidiIterator, class Allocator, class charT, class traits>
bool regex_search(BidiIterator first, BidiIterator last,
                  match_results<BidiIterator, Allocator> &m,
                  const basic_regex<charT, traits> &e,
                  match_flag_type flags = match_default) {
  return regex_search(first, last, m, e, flags, first);
}

template <class BidiIterator, class Allocator, class charT, class traits>
bool regex_search(BidiIterator first, BidiIterator last,
                  match_results<BidiIterator, Allocator> &m,
                  const basic_regex<charT, traits> &e, match_flag_type flags,
                  BidiIterator base) {
  if (e.flags() & regex_constants::failbit)
    return false;

  BOOST_REGEX_DETAIL_NS::perl_matcher<BidiIterator, Allocator, traits> matcher(
      first, last, m, e, flags, base);
  return BOOST_REGEX_DETAIL_NS::factory_find(matcher);
}

} // namespace boost

// regex_iterator
namespace boost {

template <class BidirectionalIterator, class charT, class traits>
class regex_iterator_implementation {
  typedef basic_regex<charT, traits> regex_type;

  match_results<BidirectionalIterator> what; // current match
  BidirectionalIterator base;                // start of sequence
  BidirectionalIterator end;                 // end of sequence
  const regex_type re;                       // the expression
  match_flag_type flags;                     // flags for matching

public:
  regex_iterator_implementation(const regex_type *p, BidirectionalIterator last,
                                match_flag_type f)
      : base(), end(last), re(*p), flags(f) {}
  regex_iterator_implementation(const regex_iterator_implementation &other)
      : what(other.what), base(other.base), end(other.end), re(other.re),
        flags(other.flags) {}
  bool init(BidirectionalIterator first) {
    base = first;
    return regex_search(first, end, what, re, flags);
  }
  bool compare(const regex_iterator_implementation &that) {
    if (this == &that)
      return true;
    return (&re.get_data() == &that.re.get_data()) && (end == that.end) &&
           (flags == that.flags) && (what[0].first == that.what[0].first) &&
           (what[0].second == that.what[0].second);
  }
  const match_results<BidirectionalIterator> &get() { return what; }
  bool next() {
    //if(what.prefix().first != what[0].second)
    //   flags |= match_prev_avail;
    BidirectionalIterator next_start = what[0].second;
    match_flag_type f(flags);
    if (!what.length())
      f |= regex_constants::match_not_initial_null;
    //if(base != next_start)
    //   f |= regex_constants::match_not_bob;
    bool result = regex_search(next_start, end, what, re, f, base);
    if (result)
      what.set_base(base);
    return result;
  }

private:
  regex_iterator_implementation &
  operator=(const regex_iterator_implementation &);
};

template <class BidirectionalIterator,
          class charT =
              typename std::iterator_traits<BidirectionalIterator>::value_type,
          class traits = regex_traits<charT>>
class regex_iterator {
private:
  typedef regex_iterator_implementation<BidirectionalIterator, charT, traits>
      impl;
  typedef std::shared_ptr<impl> pimpl;

public:
  typedef basic_regex<charT, traits> regex_type;
  typedef match_results<BidirectionalIterator> value_type;
  typedef typename std::iterator_traits<BidirectionalIterator>::difference_type
      difference_type;
  typedef const value_type *pointer;
  typedef const value_type &reference;
  typedef std::forward_iterator_tag iterator_category;

  regex_iterator() {}
  regex_iterator(BidirectionalIterator a, BidirectionalIterator b,
                 const regex_type &re, match_flag_type m = match_default)
      : pdata(new impl(&re, b, m)) {
    if (!pdata->init(a)) {
      pdata.reset();
    }
  }
  regex_iterator(const regex_iterator &that) : pdata(that.pdata) {}
  regex_iterator &operator=(const regex_iterator &that) {
    pdata = that.pdata;
    return *this;
  }
  bool operator==(const regex_iterator &that) const {
    if ((pdata.get() == 0) || (that.pdata.get() == 0))
      return pdata.get() == that.pdata.get();
    return pdata->compare(*(that.pdata.get()));
  }
  bool operator!=(const regex_iterator &that) const { return !(*this == that); }
  const value_type &operator*() const { return pdata->get(); }
  const value_type *operator->() const { return &(pdata->get()); }
  regex_iterator &operator++() {
    cow();
    if (0 == pdata->next()) {
      pdata.reset();
    }
    return *this;
  }
  regex_iterator operator++(int) {
    regex_iterator result(*this);
    ++(*this);
    return result;
  }

private:
  pimpl pdata;

  void cow() {
    // copy-on-write
    if (pdata.get() && (pdata.use_count() > 1)) {
      pdata.reset(new impl(*(pdata.get())));
    }
  }
};

typedef regex_iterator<const char *> cregex_iterator;

} // namespace boost

// regex_replace
namespace boost {

template <class OutputIterator, class BidirectionalIterator, class traits,
          class charT, class Formatter>
OutputIterator regex_replace(OutputIterator out, BidirectionalIterator first,
                             BidirectionalIterator last,
                             const basic_regex<charT, traits> &e, Formatter fmt,
                             match_flag_type flags = match_default) {
  regex_iterator<BidirectionalIterator, charT, traits> i(first, last, e, flags);
  regex_iterator<BidirectionalIterator, charT, traits> j;
  if (i == j) {
    out = std::copy(first, last, out);
  } else {
    BidirectionalIterator last_m(first);
    while (i != j) {
      out = std::copy(i->prefix().first, i->prefix().second, out);
      out = i->format(out, fmt, e);
      last_m = (*i)[0].second;
      ++i;
    }
    out = std::copy(last_m, last, out);
  }
  return out;
}

} // namespace boost

#endif // BOOST_REGEX_V5_REGEX_ALGORITHMS_HPP
