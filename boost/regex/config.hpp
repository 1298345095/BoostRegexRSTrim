/*
 * Copyright (c) 1998-2002 John Maddock
 * Distributed under the Boost Software License, Version 1.0.
 * See LICENSE_1_0.txt or http://www.boost.org/LICENSE_1_0.txt.
 */

#ifndef BOOST_REGEX_CONFIG_HPP
#define BOOST_REGEX_CONFIG_HPP

// Project configuration: C++17, standalone headers, exceptions, threaded
// caches and char byte ranges. This is not a general-purpose Boost install.
#include <cassert>
#define BOOST_REGEX_ASSERT(x) assert(x)

#define BOOST_REGEX_MAX_RECURSION_DEPTH 100
#define BOOST_REGEX_MAX_STATE_COUNT 100000000
#define BOOST_REGEX_BLOCKSIZE 4096
#define BOOST_REGEX_MAX_BLOCKS 1024
#define BOOST_REGEX_MAX_CACHE_BLOCKS 16

#define BOOST_REGEX_DETAIL_NS re_detail_600

#if defined(_MSC_VER) && !defined(__clang__) && !defined(__GNUC__)
#define BOOST_REGEX_MSVC _MSC_VER
#endif

// Preserve the upstream default locale choice, including GCC on Windows.
#if defined(__GNUC__) && !defined(_MSC_VER) && defined(_WIN32)
#define BOOST_REGEX_NO_W32
#endif
#if defined(_WIN32) && !defined(BOOST_REGEX_NO_W32)
#define BOOST_REGEX_USE_WIN32_LOCALE
#endif

#endif
