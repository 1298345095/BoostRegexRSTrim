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
  *   FILE         regex_fwd.cpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Forward declares boost::basic_regex<> and
  *                associated typedefs.
  */

#ifndef BOOST_REGEX_FWD_HPP
#define BOOST_REGEX_FWD_HPP

#ifndef BOOST_REGEX_CONFIG_HPP
#include <boost/regex/config.hpp>
#endif

namespace boost {

template <class charT> class cpp_regex_traits;
template <class charT> class w32_regex_traits;

#ifdef BOOST_REGEX_USE_WIN32_LOCALE
template <class charT, class implementationT = w32_regex_traits<charT>>
struct regex_traits;
#else
template <class charT, class implementationT = cpp_regex_traits<charT>>
struct regex_traits;
#endif

template <class charT, class traits = regex_traits<charT>> class basic_regex;

typedef basic_regex<char, regex_traits<char>> regex;

} // namespace boost

#endif
