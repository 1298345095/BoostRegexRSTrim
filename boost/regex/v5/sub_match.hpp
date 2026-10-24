/*
 *
 * Copyright (c) 1998-2002
 * John Maddock
 *
 * Use, modification and distribution are subject to the 
 * Boost Software License, Version 1.0. (See accompanying file 
 * LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org for most recent version.
  *   FILE         sub_match.cpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Declares template class sub_match.
  */

#ifndef BOOST_REGEX_V5_SUB_MATCH_HPP
#define BOOST_REGEX_V5_SUB_MATCH_HPP

#include <iterator>
#include <utility>

namespace boost {

template <class BidiIterator>
struct sub_match : public std::pair<BidiIterator, BidiIterator> {
  typedef typename std::iterator_traits<BidiIterator>::value_type value_type;
  typedef typename std::iterator_traits<BidiIterator>::difference_type
      difference_type;
  typedef BidiIterator iterator_type;
  typedef BidiIterator iterator;
  typedef BidiIterator const_iterator;

  bool matched;

  sub_match() : std::pair<BidiIterator, BidiIterator>(), matched(false) {}
  sub_match(BidiIterator i)
      : std::pair<BidiIterator, BidiIterator>(i, i), matched(false) {}
  difference_type length() const {
    difference_type n = matched ? std::distance((BidiIterator)this->first,
                                                (BidiIterator)this->second)
                                : 0;
    return n;
  }
  sub_match(const sub_match &that, bool = true)
      : std::pair<BidiIterator, BidiIterator>(that), matched(that.matched) {}
  sub_match &operator=(const sub_match &that) {
    this->first = that.first;
    this->second = that.second;
    matched = that.matched;
    return *this;
  }
  //
  // Make this type a range, for both Boost.Range, and C++11:
  //
  BidiIterator begin() const { return this->first; }
  BidiIterator end() const { return this->second; }
};

} // namespace boost

#endif
