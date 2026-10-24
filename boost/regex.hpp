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
  *   FILE         regex.cpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: Declares boost::basic_regex<> and associated
  *                functions and classes. This header is the main
  *                entry point for the template regex code.
  */

#ifndef BOOST_RE_REGEX_HPP
#define BOOST_RE_REGEX_HPP

// what follows is all C++ don't include in C builds!!

#include <boost/regex/config.hpp>
#include <boost/regex_fwd.hpp>
#include <boost/regex/regex_traits.hpp>
#include <boost/regex/v5/error_type.hpp>
#include <boost/regex/v5/match_flags.hpp>
#include <boost/regex/v5/regex_raw_buffer.hpp>
#include <boost/regex/pattern_except.hpp>
#include <boost/regex/v5/states.hpp>
#include <boost/regex/v5/regbase.hpp>
#include <boost/regex/v5/basic_regex.hpp>
#include <boost/regex/v5/basic_regex_creator.hpp>
#include <boost/regex/v5/basic_regex_parser.hpp>
#include <boost/regex/v5/sub_match.hpp>
#include <boost/regex/v5/regex_format.hpp>
#include <boost/regex/v5/match_results.hpp>
#include <boost/regex/v5/perl_matcher.hpp>

namespace boost {

typedef match_results<const char *> cmatch;
typedef match_results<std::string::const_iterator> smatch;

} // namespace boost

#include <boost/regex/v5/regex_algorithms.hpp>

#endif // include
