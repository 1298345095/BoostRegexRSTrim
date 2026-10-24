/*
 *
 * Copyright (c) 2003
 * John Maddock
 *
 * Use, modification and distribution are subject to the
 * Boost Software License, Version 1.0. (See accompanying file
 * LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org for most recent version.
  *   FILE         regex_traits.hpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Declares regular expression traits classes.
  */

#ifndef BOOST_REGEX_TRAITS_HPP
#define BOOST_REGEX_TRAITS_HPP

#include <boost/regex/config.hpp>
#include <boost/regex/v5/syntax_type.hpp>
#include <boost/regex/v5/error_type.hpp>
#include <boost/regex/v5/regex_traits_defaults.hpp>
#include <boost/regex/v5/cpp_regex_traits.hpp>
#if defined(_WIN32) && !defined(BOOST_REGEX_NO_W32)
#include <boost/regex/v5/w32_regex_traits.hpp>
#endif
#include <boost/regex_fwd.hpp>

namespace boost {

template <class charT, class implementationT>
struct regex_traits : public implementationT {
  regex_traits() : implementationT() {}
};

// Both retained platform traits provide the interfaces used by the runtime.
template <class BaseT> struct regex_traits_wrapper : public BaseT {
  regex_traits_wrapper() {}

private:
  regex_traits_wrapper(const regex_traits_wrapper &);
  regex_traits_wrapper &operator=(const regex_traits_wrapper &);
};

} // namespace boost

#endif // include
