/*
 * Copyright (c) 2002
 * John Maddock
 *
 * Use, modification and distribution are subject to the 
 * Boost Software License, Version 1.0. (See accompanying file 
 * LICENSE_1_0.txt or copy at http://www.boost.org/LICENSE_1_0.txt)
 *
 */

/*
  *   LOCATION:    see http://www.boost.org for most recent version.
  *   FILE         mem_block_cache.hpp
  *   VERSION      see <boost/version.hpp>
  *   DESCRIPTION: memory block cache used by the non-recursive matcher.
  */

#ifndef BOOST_REGEX_V5_MEM_BLOCK_CACHE_HPP
#define BOOST_REGEX_V5_MEM_BLOCK_CACHE_HPP

#include <boost/regex/config.hpp>
#include <atomic>
#include <new>
#include <mutex>

namespace boost {
namespace BOOST_REGEX_DETAIL_NS {

#if ATOMIC_POINTER_LOCK_FREE == 2 /* lock free implementation */
struct mem_block_cache {
  std::atomic<void *> cache[BOOST_REGEX_MAX_CACHE_BLOCKS];

  ~mem_block_cache() {
    for (size_t i = 0; i < BOOST_REGEX_MAX_CACHE_BLOCKS; ++i) {
      if (cache[i].load())
        ::operator delete(cache[i].load());
    }
  }
  void *get() {
    for (size_t i = 0; i < BOOST_REGEX_MAX_CACHE_BLOCKS; ++i) {
      void *p = cache[i].load();
      if (p != NULL) {
        if (cache[i].compare_exchange_strong(p, NULL))
          return p;
      }
    }
    return ::operator new(BOOST_REGEX_BLOCKSIZE);
  }
  void put(void *ptr) {
    for (size_t i = 0; i < BOOST_REGEX_MAX_CACHE_BLOCKS; ++i) {
      void *p = cache[i].load();
      if (p == NULL) {
        if (cache[i].compare_exchange_strong(p, ptr))
          return;
      }
    }
    ::operator delete(ptr);
  }

  static mem_block_cache &instance() {
    static mem_block_cache block_cache = {{{nullptr}}};
    return block_cache;
  }
};

#else /* lock-based implementation */

struct mem_block_node {
  mem_block_node *next;
};

struct mem_block_cache {
  // this member has to be statically initialsed:
  mem_block_node *next{nullptr};
  unsigned cached_blocks{0};
  std::mutex mut;

  ~mem_block_cache() {
    while (next) {
      mem_block_node *old = next;
      next = next->next;
      ::operator delete(old);
    }
  }
  void *get() {
    std::lock_guard<std::mutex> g(mut);
    if (next) {
      mem_block_node *result = next;
      next = next->next;
      --cached_blocks;
      return result;
    }
    return ::operator new(BOOST_REGEX_BLOCKSIZE);
  }
  void put(void *p) {
    std::lock_guard<std::mutex> g(mut);
    if (cached_blocks >= BOOST_REGEX_MAX_CACHE_BLOCKS) {
      ::operator delete(p);
    } else {
      mem_block_node *old = static_cast<mem_block_node *>(p);
      old->next = next;
      next = old;
      ++cached_blocks;
    }
  }
  static mem_block_cache &instance() {
    static mem_block_cache block_cache;
    return block_cache;
  }
};
#endif

inline void *get_mem_block() { return mem_block_cache::instance().get(); }

inline void put_mem_block(void *p) { mem_block_cache::instance().put(p); }

} // namespace BOOST_REGEX_DETAIL_NS
} // namespace boost

#endif
