// Test-only byte-preserving driver for the repository's retained C++ engine.
// Distributed under the Boost Software License, Version 1.0.
#include <algorithm>
#include <climits>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cstdlib>
#include <exception>
#include <iostream>
#include <iterator>
#include <locale>
#include <limits>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <Psapi.h>
#else
#include <sys/resource.h>
#endif

#include <boost/regex.hpp>

namespace {
namespace detail = boost::BOOST_REGEX_DETAIL_NS;

class driver_io_error : public std::runtime_error {
public:
  using std::runtime_error::runtime_error;
};

#ifdef BOOST_REGEX_USE_WIN32_LOCALE
unsigned long selected_lcid = 0;
#endif

struct reference_traits : boost::regex_traits<char> {
  reference_traits() {
#ifdef BOOST_REGEX_USE_WIN32_LOCALE
    if (selected_lcid) imbue(selected_lcid);
#endif
  }
};

using reference_regex = boost::basic_regex<char, reference_traits>;
using reference_iterator = boost::regex_iterator<const char *, char, reference_traits>;

std::string hex(std::string_view bytes) {
  static constexpr char digits[] = "0123456789abcdef";
  std::string result;
  result.reserve(bytes.size() * 2);
  for (unsigned char byte : bytes) {
    result += digits[byte >> 4];
    result += digits[byte & 15];
  }
  return result;
}

unsigned nibble(char c) {
  if (c >= '0' && c <= '9') return static_cast<unsigned>(c - '0');
  if (c >= 'a' && c <= 'f') return static_cast<unsigned>(c - 'a' + 10);
  if (c >= 'A' && c <= 'F') return static_cast<unsigned>(c - 'A' + 10);
  throw std::invalid_argument("Invalid hexadecimal request byte");
}

std::string unhex(std::string_view value) {
  if (value.size() % 2) throw std::invalid_argument("Odd hexadecimal request length");
  std::string result;
  result.reserve(value.size() / 2);
  for (std::size_t i = 0; i < value.size(); i += 2)
    result += static_cast<char>((nibble(value[i]) << 4) | nibble(value[i + 1]));
  return result;
}

std::vector<std::string> fields(std::string_view line) {
  std::vector<std::string> result;
  std::size_t start = 0;
  for (;;) {
    const auto end = line.find('\t', start);
    result.emplace_back(line.substr(start, end - start));
    if (end == std::string::npos) break;
    start = end + 1;
  }
  if (result.size() != 4) throw std::invalid_argument("Expected four tab-separated fields");
  return result;
}

void error(std::ostream &out, const char *phase, std::string_view message,
           const boost::regex_error *detail = nullptr) {
  out << R"({"ok":false,"phase":")" << phase << R"(","code":)";
  if (detail) out << static_cast<unsigned>(detail->code());
  else out << "null";
  out << R"(,"position":)";
  if (detail) out << detail->position();
  else out << "null";
  out << R"(,"message_hex":")" << hex(message) << R"("})";
}

std::string raw_message(const boost::regex_error &ex) {
#ifdef BOOST_REGEX_REFERENCE_RAW_OBSERVER
  return ex.raw_message();
#else
  return ex.what();
#endif
}

void match(std::ostream &out, const boost::cmatch &found, const char *base) {
  out << R"({"span":[)" << found[0].first - base << ',' << found[0].second - base
      << R"(],"captures":[)";
  for (std::size_t i = 0; i < found.size(); ++i) {
    if (i) out << ',';
    if (found[i].matched)
      out << '[' << found[i].first - base << ',' << found[i].second - base << ']';
    else out << "null";
  }
  out << "]}";
}

void accounting(std::ostream &out, const std::string &operation,
                const std::string &pattern_hex, const std::string &input_hex);

void execute(std::ostream &out, const std::vector<std::string> &request) {
  const auto &operation = request[0];
  if (operation == "accounting_search" || operation == "accounting_full") {
    (void)unhex(request[3]);
    accounting(out, operation == "accounting_full" ? "full" : "search",
               request[1], request[2]);
    return;
  }
  if (operation != "compile" && operation != "search" && operation != "full" &&
      operation != "find_all" && operation != "replace")
    throw std::invalid_argument("Unknown operation");
  const auto pattern = unhex(request[1]);
  const auto input = unhex(request[2]);
  const auto replacement = unhex(request[3]);
  reference_regex expression;
  try {
    expression.assign(pattern.data(), pattern.data() + pattern.size());
  } catch (const boost::regex_error &ex) {
    error(out, "compile", raw_message(ex), &ex);
    return;
  } catch (const std::exception &ex) {
    error(out, "compile", ex.what());
    return;
  }
  if (operation == "compile") {
    out << R"({"ok":true,"mark_count":)" << expression.mark_count() << '}';
    return;
  }
  // Buffer the entire answer so failures cannot leave a partial JSON response.
  std::ostringstream result;
  try {
    const char *first = input.data();
    const char *last = first + input.size();
    if (operation == "replace") {
      std::string output;
      boost::regex_replace(std::back_inserter(output), first, last, expression,
                           replacement);
      result << R"({"ok":true,"output_hex":")" << hex(output) << R"("})";
    } else if (operation == "find_all") {
      result << R"({"ok":true,"matches":[)";
      bool comma = false;
      for (reference_iterator i(first, last, expression), end; i != end; ++i) {
        if (comma) result << ',';
        match(result, *i, first);
        comma = true;
      }
      result << "]}";
    } else {
      boost::cmatch found;
      const bool matched = operation == "full"
          ? boost::regex_match(first, last, found, expression)
          : boost::regex_search(first, last, found, expression);
      result << R"({"ok":true,"match":)";
      if (matched) match(result, found, first);
      else result << "null";
      result << '}';
    }
    out << result.str();
  } catch (const boost::regex_error &ex) {
    error(out, "execute", raw_message(ex), &ex);
  } catch (const std::exception &ex) {
    error(out, "execute", ex.what());
  }
}

void environment() {
  reference_regex expression;
  const std::string pattern = "a";
  expression.assign(pattern.data(), pattern.data() + pattern.size());
  std::cout << R"({"pointer_bits":)" << sizeof(void *) * 8
            << R"(,"char_signed":)" << (std::numeric_limits<char>::is_signed ? "true" : "false");
#ifdef BOOST_REGEX_USE_WIN32_LOCALE
  std::cout << R"(,"traits":"win32","locale_id":)"
            << expression.get_traits().getloc();
#else
  std::cout << R"(,"traits":"cpp_locale","locale_name_hex":")"
            << hex(expression.get_traits().getloc().name()) << '"';
#endif
#ifdef _MSC_VER
  std::cout << R"(,"msvc_version":)" << _MSC_VER;
#endif
#ifdef __clang_version__
  std::cout << R"(,"clang_version_hex":")" << hex(__clang_version__) << '"';
#elif defined(__VERSION__)
  std::cout << R"(,"compiler_version_hex":")" << hex(__VERSION__) << '"';
#endif
  std::cout << R"(,"cplusplus":)" << __cplusplus
            << R"(,"state_limit":)" << BOOST_REGEX_MAX_STATE_COUNT
            << R"(,"block_size":)" << BOOST_REGEX_BLOCKSIZE
            << R"(,"max_blocks":)" << BOOST_REGEX_MAX_BLOCKS
            << R"(,"creator_depth":)" << BOOST_REGEX_MAX_RECURSION_DEPTH;
  std::cout << R"(,"saved_state_bytes":{"base":)" << sizeof(detail::saved_state)
            << R"(,"capture":)" << sizeof(detail::saved_matched_paren<const char *>)
            << R"(,"position":)" << sizeof(detail::saved_position<const char *>)
            << R"(,"assertion":)" << sizeof(detail::saved_assertion<const char *>)
            << R"(,"repeater":)" << sizeof(detail::saved_repeater<const char *>)
            << R"(,"extra_block":)" << sizeof(detail::saved_extra_block)
            << R"(,"single_repeat":)" << sizeof(detail::saved_single_repeat<const char *>)
            << R"(,"recursion":)" << sizeof(detail::saved_recursion<boost::cmatch>)
            << R"(,"case":)" << sizeof(detail::saved_change_case) << '}';
  std::cout << R"(,"match_results_bytes":)" << sizeof(boost::cmatch);
#ifdef BOOST_REGEX_REFERENCE_RAW_OBSERVER
  std::cout << R"(,"raw_error_observer":true)";
#else
  std::cout << R"(,"raw_error_observer":false)";
#endif
  std::cout << "}\n";
}

bool has_alternative(detail::syntax_element_type type) {
  return type == detail::syntax_element_jump || type == detail::syntax_element_alt ||
         type == detail::syntax_element_rep || type == detail::syntax_element_dot_rep ||
         type == detail::syntax_element_char_rep || type == detail::syntax_element_short_set_rep ||
         type == detail::syntax_element_long_set_rep || type == detail::syntax_element_recurse;
}

using program_states = std::map<std::uintptr_t, const detail::re_syntax_base *>;

program_states collect_program_states(const detail::re_syntax_base *first) {
  program_states states;
  std::vector pending{first};
  while (!pending.empty()) {
    const auto *state = pending.back();
    pending.pop_back();
    if (!state || !states.emplace(reinterpret_cast<std::uintptr_t>(state), state).second)
      continue;
    pending.push_back(state->next.p);
    if (has_alternative(state->type))
      pending.push_back(static_cast<const detail::re_jump *>(state)->alt.p);
  }
  return states;
}

void program_address(std::ostream &out, const detail::re_syntax_base *state,
                     std::uintptr_t origin) {
  if (state) out << reinterpret_cast<std::uintptr_t>(state) - origin;
  else out << "null";
}

void program_byte_map(std::ostream &out, const unsigned char *map) {
  out << hex(std::string_view(reinterpret_cast<const char *>(map), 256));
}

void program_strings(std::ostream &out, const char *&data, unsigned count,
                     unsigned minimum_length) {
  for (unsigned i = 0; i < count; ++i) {
    if (i) out << ',';
    const char *start = data;
    // Range endpoints and equivalence keys retain a leading literal NUL in
    // addition to their terminating NUL; single strings use ordinary strlen.
    data += minimum_length;
    while (*data) ++data;
    out << '"' << hex(std::string_view(start, static_cast<std::size_t>(data - start))) << '"';
    ++data;
  }
}

void program_long_set(std::ostream &out, const detail::re_syntax_base *state) {
  using set_type = detail::re_set_long<reference_traits::char_class_type>;
  const auto *set = static_cast<const set_type *>(state);
  out << R"(,"classes":)" << set->cclasses
      << R"(,"negated_classes":)" << set->cnclasses
      << R"(,"negated":)" << (set->isnot ? "true" : "false")
      << R"(,"singleton":)" << (set->singleton ? "true" : "false")
      << R"(,"singles_hex":[)";
  const char *data = reinterpret_cast<const char *>(set + 1);
  program_strings(out, data, set->csingles, 0);
  out << R"(],"range_endpoints_hex":[)";
  program_strings(out, data, 2 * set->cranges, 1);
  out << R"(],"equivalence_keys_hex":[)";
  program_strings(out, data, set->cequivalents, 1);
  out << ']';
}

void program_repeat(std::ostream &out, const detail::re_syntax_base *state) {
  const auto *repeat = static_cast<const detail::re_repeat *>(state);
  out << R"(,"min":)" << repeat->min << R"(,"max":)" << repeat->max
      << R"(,"state_id":)" << repeat->state_id
      << R"(,"leading":)" << (repeat->leading ? "true" : "false")
      << R"(,"greedy":)" << (repeat->greedy ? "true" : "false");
}

void program_state_data(std::ostream &out, const detail::re_syntax_base *state) {
  switch (state->type) {
  case detail::syntax_element_startmark:
  case detail::syntax_element_endmark:
  case detail::syntax_element_backref: {
    const auto *brace = static_cast<const detail::re_brace *>(state);
    out << R"(,"index":)" << brace->index
        << R"(,"icase":)" << (brace->icase ? "true" : "false");
    break;
  }
  case detail::syntax_element_backstep:
  case detail::syntax_element_assert_backref:
    out << R"(,"index":)" << static_cast<const detail::re_brace *>(state)->index;
    break;
  case detail::syntax_element_literal: {
    const auto *literal = static_cast<const detail::re_literal *>(state);
    out << R"(,"literal_hex":")"
        << hex(std::string_view(reinterpret_cast<const char *>(literal + 1), literal->length))
        << '"';
    break;
  }
  case detail::syntax_element_wild:
    out << R"(,"mask":)" << static_cast<unsigned>(static_cast<const detail::re_dot *>(state)->mask);
    break;
  case detail::syntax_element_toggle_case:
    out << R"(,"icase":)" << (static_cast<const detail::re_case *>(state)->icase ? "true" : "false");
    break;
  case detail::syntax_element_set:
    out << R"(,"map_hex":")";
    program_byte_map(out, static_cast<const detail::re_set *>(state)->_map);
    out << '"';
    break;
  case detail::syntax_element_long_set:
    program_long_set(out, state);
    break;
  case detail::syntax_element_rep:
  case detail::syntax_element_dot_rep:
  case detail::syntax_element_char_rep:
  case detail::syntax_element_short_set_rep:
  case detail::syntax_element_long_set_rep:
    program_repeat(out, state);
    [[fallthrough]];
  case detail::syntax_element_alt: {
    const auto *alt = static_cast<const detail::re_alt *>(state);
    out << R"(,"can_be_null":)" << alt->can_be_null << R"(,"map_hex":")";
    program_byte_map(out, alt->_map);
    out << '"';
    break;
  }
  case detail::syntax_element_recurse:
    out << R"(,"state_id":)" << static_cast<const detail::re_recurse *>(state)->state_id;
    break;
  case detail::syntax_element_commit:
    out << R"(,"action":)" << static_cast<const detail::re_commit *>(state)->action;
    break;
  default:
    break;
  }
}

void program_state(std::ostream &out, const detail::re_syntax_base *state,
                   std::uintptr_t origin) {
  out << R"({"id":)";
  program_address(out, state, origin);
  out << R"(,"kind":)" << state->type << R"(,"next":)";
  program_address(out, state->next.p, origin);
  if (has_alternative(state->type)) {
    out << R"(,"alt":)";
    program_address(out, static_cast<const detail::re_jump *>(state)->alt.p, origin);
  }
  program_state_data(out, state);
  out << '}';
}

void program(const std::string &encoded_pattern) {
  try {
    const std::string pattern = unhex(encoded_pattern);
    reference_regex expression;
    expression.assign(pattern.data(), pattern.data() + pattern.size());
    const auto *first = expression.get_first_state();
    const auto origin = reinterpret_cast<std::uintptr_t>(first);
    const auto states = collect_program_states(first);
    std::cout << R"({"ok":true,"pattern_len":)" << expression.size()
              << R"(,"mark_count":)" << expression.mark_count()
              << R"(,"restart":)" << expression.get_restart_type()
              << R"(,"can_be_null":)" << (expression.can_be_null() ? "true" : "false")
              << R"(,"start_map_hex":")";
    program_byte_map(std::cout, expression.get_map());
    std::cout << R"(","states":[)";
    bool comma = false;
    for (const auto &entry : states) {
      if (comma) std::cout << ',';
      comma = true;
      program_state(std::cout, entry.second, origin);
    }
    std::cout << "]}\n";
  } catch (const boost::regex_error &ex) {
    error(std::cout, "compile", raw_message(ex), &ex);
    std::cout << '\n';
  } catch (const std::exception &ex) {
    // The protocol boundary must contain allocation and standard-library failures.
    error(std::cout, "protocol", ex.what());
    std::cout << '\n';
  }
}

std::uint64_t peak_rss_bytes() {
#ifdef _WIN32
  PROCESS_MEMORY_COUNTERS counters{};
  if (!K32GetProcessMemoryInfo(GetCurrentProcess(), &counters, static_cast<DWORD>(sizeof(counters))))
    throw driver_io_error("Unable to read process peak working set");
  return static_cast<std::uint64_t>(counters.PeakWorkingSetSize);
#else
  rusage usage{};
  if (getrusage(RUSAGE_SELF, &usage) != 0)
    throw driver_io_error("Unable to read process peak resident set");
#ifdef __APPLE__
  return static_cast<std::uint64_t>(usage.ru_maxrss);
#else
  return static_cast<std::uint64_t>(usage.ru_maxrss) * 1024;
#endif
#endif
}

void mix_checksum(std::uint64_t &checksum, std::uint64_t value) {
  // This shared noncryptographic checksum only consumes benchmark results.
  checksum = (checksum * UINT64_C(1099511628211)) ^ value;
}

void consume_match(std::uint64_t &checksum, bool matched,
                   const boost::cmatch &found, const char *first) {
  mix_checksum(checksum, matched ? 1 : 0);
  if (!matched) return;
  mix_checksum(checksum, found.size());
  for (std::size_t i = 0; i < found.size(); ++i) {
    mix_checksum(checksum, found[i].matched ? 1 : 0);
    if (found[i].matched) {
      mix_checksum(checksum, static_cast<std::uint64_t>(found[i].first - first));
      mix_checksum(checksum, static_cast<std::uint64_t>(found[i].second - first));
    }
  }
}

void benchmark_execution(std::string_view operation, const reference_regex &expression,
                         const std::string &input, const std::string &replacement,
                         std::uint64_t &checksum) {
  const char *first = input.data();
  const char *last = first + input.size();
  if (operation == "replace") {
    std::string output;
    boost::regex_replace(std::back_inserter(output), first, last, expression, replacement);
    mix_checksum(checksum, output.size());
    for (unsigned char byte : output) mix_checksum(checksum, byte);
    return;
  }
  boost::cmatch found;
  const bool matched = operation == "full"
      ? boost::regex_match(first, last, found, expression)
      : boost::regex_search(first, last, found, expression);
  consume_match(checksum, matched, found, first);
}

void benchmark(bool measured, const std::string &operation,
               const std::string &pattern_hex, const std::string &input_hex,
               const std::string &template_hex, const std::string &count) {
  const char *phase = "protocol";
  try {
#ifdef BOOST_REGEX_REFERENCE_RAW_OBSERVER
    if (measured) throw std::invalid_argument("Benchmarks require the uninstrumented --baseline build");
#endif
    if (operation != "compile" && operation != "search" && operation != "full" && operation != "replace")
      throw std::invalid_argument("Unknown benchmark operation");
    std::size_t consumed = 0;
    const std::uint64_t iterations = std::stoull(count, &consumed);
    if (consumed != count.size() || iterations == 0 || count[0] == '-')
      throw std::invalid_argument("Benchmark iterations must be a positive integer");
    const auto pattern = unhex(pattern_hex);
    const auto input = unhex(input_hex);
    const auto replacement = unhex(template_hex);
    reference_regex expression;
    phase = "compile";
    if (operation != "compile")
      expression.assign(pattern.data(), pattern.data() + pattern.size());
    std::uint64_t checksum = UINT64_C(14695981039346656037);
    const auto started = measured ? std::chrono::steady_clock::now() : std::chrono::steady_clock::time_point{};
    for (std::uint64_t iteration = 0; iteration < iterations; ++iteration) {
      if (operation == "compile") {
        reference_regex compiled;
        compiled.assign(pattern.data(), pattern.data() + pattern.size());
        mix_checksum(checksum, compiled.mark_count());
        mix_checksum(checksum, compiled.size());
      } else {
        phase = "execute";
        benchmark_execution(operation, expression, input, replacement, checksum);
      }
    }
    const auto finished = measured ? std::chrono::steady_clock::now() : std::chrono::steady_clock::time_point{};
    std::ostringstream output;
    output << R"({"ok":true,"operation":")" << operation << R"(","iterations":)"
           << iterations << R"(,"checksum":)" << checksum
           << R"(,"measured":)" << (measured ? "true" : "false");
    if (measured) {
      const auto elapsed = std::chrono::duration_cast<std::chrono::nanoseconds>(finished - started).count();
      output << R"(,"elapsed_ns":)" << elapsed << R"(,"peak_rss_bytes":)" << peak_rss_bytes();
    }
    output << "}\n";
    std::cout << output.str();
  } catch (const boost::regex_error &ex) {
    error(std::cout, phase, raw_message(ex), &ex);
    std::cout << '\n';
  } catch (const std::exception &ex) {
    error(std::cout, phase, ex.what());
    std::cout << '\n';
  }
}

#ifdef BOOST_REGEX_REFERENCE_ACCOUNTING_OBSERVER
using observed_matcher = detail::perl_matcher<
    const char *, boost::cmatch::allocator_type, reference_traits>;

bool accounting_match(std::ostream &out, observed_matcher &matcher,
                      const boost::cmatch &found, const char *first,
                      std::string_view operation) {
  bool counters_initialized = false;
  try {
    const bool matched = operation == "full" ? matcher.match() : matcher.find();
    counters_initialized = true;
    out << R"({"ok":true,"match":)";
    if (matched) match(out, found, first);
    else out << "null";
    out << '}';
  } catch (const boost::regex_error &ex) {
    // All retained computation-limit failures happen after counters initialize.
    counters_initialized = true;
    error(out, "execute", raw_message(ex), &ex);
  } catch (const std::exception &ex) {
    // Other failures may occur before the observer counters are initialized.
    error(out, "execute", ex.what());
  }
  return counters_initialized;
}
#endif

void accounting(std::ostream &out, const std::string &operation,
                const std::string &pattern_hex, const std::string &input_hex) {
  const char *phase = "protocol";
  try {
#ifdef BOOST_REGEX_REFERENCE_ACCOUNTING_OBSERVER
    if (operation != "search" && operation != "full")
      throw std::invalid_argument("Accounting operation must be search or full");
    const auto pattern = unhex(pattern_hex);
    const auto input = unhex(input_hex);
    reference_regex expression;
    phase = "compile";
    expression.assign(pattern.data(), pattern.data() + pattern.size());
    boost::cmatch found;
    const char *first = input.data();
    observed_matcher matcher(first, first + input.size(), found, expression,
                             boost::match_default, first);
    std::ostringstream result;
    const bool counters_initialized = accounting_match(result, matcher, found, first, operation);
    const auto json = result.str();
    out << json.substr(0, json.size() - 1) << R"(,"accounting":)";
    if (counters_initialized) {
      out << R"({"state_count":)" << matcher.reference_state_count()
          << R"(,"max_state_count":)" << matcher.reference_max_state_count()
          << R"(,"blocks_remaining_after_cleanup":)" << matcher.reference_used_block_count() << '}';
    } else out << "null";
    out << '}';
#else
    (void)operation;
    (void)pattern_hex;
    (void)input_hex;
    throw std::invalid_argument("Accounting requires the observed reference build");
#endif
  } catch (const boost::regex_error &ex) {
    error(out, phase, raw_message(ex), &ex);
  } catch (const std::exception &ex) {
    error(out, phase, ex.what());
  }
}
} // namespace

int main(int argc, char **argv) {
  try {
    for (int i = 1; i < argc; ++i) {
      const std::string arg = argv[i];
      if (arg == "--locale" && i + 1 < argc) {
        std::locale::global(std::locale(argv[++i]));
      } else if (arg == "--lcid" && i + 1 < argc) {
#ifdef BOOST_REGEX_USE_WIN32_LOCALE
        std::size_t consumed = 0;
        const std::string value = argv[++i];
        selected_lcid = std::stoul(value, &consumed, 0);
        if (consumed != value.size() || selected_lcid == 0)
          throw std::invalid_argument("Invalid locale identifier");
#else
        throw std::invalid_argument("--lcid requires Windows byte traits");
#endif
      } else if (arg == "--environment") {
        environment();
        return 0;
      } else if (arg == "--program" && i + 1 < argc) {
        program(argv[++i]);
        return 0;
      } else if ((arg == "--benchmark" || arg == "--benchmark-check") && i + 5 < argc) {
        benchmark(arg == "--benchmark", argv[i + 1], argv[i + 2], argv[i + 3], argv[i + 4], argv[i + 5]);
        return 0;
      } else if (arg == "--accounting" && i + 3 < argc) {
        accounting(std::cout, argv[i + 1], argv[i + 2], argv[i + 3]);
        std::cout << '\n';
        return 0;
      } else {
        throw std::invalid_argument("Usage: reference [--locale NAME | --lcid NUMBER] [--environment]");
      }
    }
    std::string line;
    while (std::getline(std::cin, line)) {
      if (!line.empty() && line.back() == '\r') line.pop_back();
      try { execute(std::cout, fields(line)); }
      catch (const std::exception &ex) { error(std::cout, "protocol", ex.what()); }
      std::cout << '\n' << std::flush;
    }
    if (std::cin.bad()) throw driver_io_error("Failed to read request stream");
  } catch (const std::exception &ex) {
    std::cerr << ex.what() << '\n';
    return 1;
  }
}
