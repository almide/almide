#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Multi-language lang-bench runner — one-shot snapshot (2026-07).
#
# Runs the mame/ai-coding-lang-bench minigit task against a single language
# with a pinned model, appending results to raw/<lang>-sonnet5.jsonl.
# One process handles one language; run several processes in parallel to
# cover the language set.
#
# Usage:
#   ruby runner_multi.rb --lang gleam --trials 20
#   ruby runner_multi.rb --lang almide --trials 10 --start 11   # shard
#   ruby runner_multi.rb --lang rust --trials 1 --dry-run
#
# Requirements:
#   - git submodule initialized (upstream/)
#   - claude CLI in PATH
#   - per-language toolchain in PATH (almide / gleam / moon / rustc / tsx / go / zig)

require 'json'
require 'fileutils'
require 'time'
require 'shellwords'
require_relative 'lib/bench_common'

SCRIPT_DIR   = File.expand_path(__dir__)
UPSTREAM_DIR = File.join(SCRIPT_DIR, 'upstream')
RAW_DIR      = File.join(SCRIPT_DIR, 'raw')
WORK_DIR     = File.join(SCRIPT_DIR, '.work')
LOGS_DIR     = File.join(SCRIPT_DIR, '.logs')
CHEATSHEET   = File.expand_path(File.join(SCRIPT_DIR, '..', '..', '..', 'docs', 'CHEATSHEET.md'))

MODEL     = 'claude-sonnet-5'
MODEL_TAG = 'sonnet5'

ALMIDE_V1_PROMPT = <<~PROMPT.strip
  Implement minigit as described in SPEC-v1.txt using Almide.
  The executable must be named 'minigit' and be runnable as ./minigit.
  Verify your implementation passes all tests by running: bash test-v1.sh
  Almide is a new language. Read CHEATSHEET.md for the complete language reference.
  Build with: almide build main.almd -o minigit
PROMPT

ALMIDE_V2_PROMPT = <<~PROMPT.strip
  Read SPEC-v2.txt and extend the existing minigit implementation
  with checkout and reset commands.
  Verify your implementation passes all tests by running: bash test-v2.sh
  Almide is a new language. Read CHEATSHEET.md for the complete language reference.
  Build with: almide build main.almd -o minigit
PROMPT

# Non-Almide languages use the upstream benchmark.rb prompts verbatim.
def generic_v1_prompt(display_name)
  "Implement minigit as described in SPEC-v1.txt using #{display_name}. " \
    "The executable must be named 'minigit' and be runnable as ./minigit. " \
    'For compiled languages, include a Makefile or build script. ' \
    'For interpreted languages, ensure the minigit file has a proper shebang line and is executable. ' \
    'Verify your implementation passes all tests by running: bash test-v1.sh'
end

GENERIC_V2_PROMPT =
  'Read SPEC-v2.txt and extend the existing minigit implementation ' \
  'with checkout and reset commands. ' \
  'Verify your implementation passes all tests by running: bash test-v2.sh'

LANGUAGES = {
  'almide' => {
    display: 'Almide', exts: %w[almd], version_cmd: 'almide --version',
    v1_prompt: ALMIDE_V1_PROMPT, v2_prompt: ALMIDE_V2_PROMPT, copy_cheatsheet: true,
  },
  'gleam' => {
    display: 'Gleam', exts: %w[gleam], version_cmd: 'gleam --version',
  },
  'moonbit' => {
    display: 'MoonBit', exts: %w[mbt], version_cmd: 'moon version',
  },
  'rust' => {
    display: 'Rust', exts: %w[rs], version_cmd: 'rustc --version',
  },
  'typescript' => {
    display: 'TypeScript', exts: %w[ts], version_cmd: 'tsx --version',
  },
  'go' => {
    display: 'Go', exts: %w[go], version_cmd: 'go version',
  },
  'zig' => {
    display: 'Zig', exts: %w[zig], version_cmd: 'zig version',
  },
}.freeze

EXCLUDE_DIR_FRAGMENTS = BENCH_EXCLUDE_DIR_FRAGMENTS

# --- Arg parsing -----------------------------------------------------------

lang = nil
trials = 1
start_override = nil
dry_run = false
i = 0
while i < ARGV.length
  case ARGV[i]
  when '--lang', '-l'   then lang = ARGV[i + 1]; i += 2
  when '--trials', '-t' then trials = ARGV[i + 1].to_i; i += 2
  when '--start', '-s'  then start_override = ARGV[i + 1].to_i; i += 2
  when '--dry-run'      then dry_run = true; i += 1
  else
    warn "unknown arg: #{ARGV[i]}"; i += 1
  end
end

abort "error: --lang required (one of: #{LANGUAGES.keys.join(', ')})" unless LANGUAGES.key?(lang)
config = LANGUAGES[lang]

# --- Preflight -------------------------------------------------------------

unless File.exist?(File.join(UPSTREAM_DIR, 'SPEC-v1.txt'))
  abort "error: upstream submodule not initialized. Run:\n  git submodule update --init #{File.join('research', 'benchmark', 'lang-bench', 'upstream')}"
end

%w[claude ruby].each do |cmd|
  abort "error: #{cmd} not found in PATH" unless system("command -v #{cmd} >/dev/null 2>&1")
end

if config[:copy_cheatsheet]
  abort "error: CHEATSHEET.md not found at #{CHEATSHEET}" unless File.exist?(CHEATSHEET)
end

FileUtils.mkdir_p([RAW_DIR, WORK_DIR, LOGS_DIR])

RAW_JSONL = File.join(RAW_DIR, "#{lang}-#{MODEL_TAG}.jsonl")
existing = File.exist?(RAW_JSONL) ? File.readlines(RAW_JSONL).map { |l| JSON.parse(l) } : []
start_trial = start_override || ((existing.map { |r| r['trial'] }.max || 0) + 1)

upstream_rev = `cd #{Shellwords.escape(UPSTREAM_DIR)} && git rev-parse --short HEAD`.strip
toolchain_ver = begin
  r = run_cmd(config[:version_cmd])
  (r[:stdout].strip.empty? ? r[:stderr].strip : r[:stdout].strip).lines.first&.strip || 'unknown'
end
claude_ver = `claude --version 2>/dev/null`.strip

puts '=' * 60
puts "lang-bench multi runner — #{lang} (#{MODEL})"
puts '=' * 60
puts "Upstream commit: #{upstream_rev}"
puts "Toolchain:       #{toolchain_ver}"
puts "Claude CLI:      #{claude_ver}"
puts "Raw file:        #{RAW_JSONL}"
puts "Existing trials: #{existing.length}"
puts "Trials to run:   #{start_trial}..#{start_trial + trials - 1}"
puts "Dry run:         #{dry_run}"
puts

# run_cmd / run_claude / run_tests / count_loc / parse_claude_json live in lib/bench_common.rb

# --- Warmup ----------------------------------------------------------------

unless dry_run
  puts '--- Warmup ---'
  warmup = File.join(WORK_DIR, ".warmup-#{lang}")
  FileUtils.rm_rf(warmup)
  FileUtils.mkdir_p(warmup)
  w = run_claude('Respond with just the word OK.', dir: warmup, model: MODEL)
  puts "  done in #{w[:elapsed]}s (success=#{w[:success]})"
  FileUtils.rm_rf(warmup)
  puts
end

# --- Trials ----------------------------------------------------------------

trials.times do |idx|
  trial = start_trial + idx
  puts '=' * 60
  puts "Trial #{trial} — #{lang} (#{idx + 1}/#{trials})"
  puts '=' * 60

  slug = "minigit-#{lang}-#{MODEL_TAG}-#{trial}"
  v1_dir = File.join(WORK_DIR, "#{slug}-v1")
  v2_dir = File.join(WORK_DIR, "#{slug}-v2")
  FileUtils.rm_rf([v1_dir, v2_dir])
  FileUtils.mkdir_p(v1_dir)

  record = {
    'language' => lang,
    'model' => MODEL,
    'trial' => trial,
    'timestamp' => Time.now.iso8601,
    'upstream_commit' => upstream_rev,
    'toolchain_version' => toolchain_ver,
    'claude_version' => claude_ver,
  }

  # Phase 1: v1
  puts "\n--- Phase 1: v1 ---"
  FileUtils.cp(File.join(UPSTREAM_DIR, 'SPEC-v1.txt'), v1_dir)
  FileUtils.cp(File.join(UPSTREAM_DIR, 'test-v1.sh'), v1_dir)
  FileUtils.cp(CHEATSHEET, v1_dir) if config[:copy_cheatsheet]

  if dry_run
    puts '  [DRY RUN]'
    record['v1_time'] = 0
  else
    v1 = run_claude(config[:v1_prompt] || generic_v1_prompt(config[:display]),
                    dir: v1_dir, model: MODEL, log_path: File.join(LOGS_DIR, "#{slug}-v1.json"))
    record['v1_time'] = v1[:elapsed]
    record['v1_claude'] = v1[:claude_data]

    puts '  Running v1 tests...'
    t1 = run_tests('test-v1.sh', dir: v1_dir)
    record['v1_pass'] = t1[:success]
    record['v1_passed_count'] = t1[:passed]
    record['v1_failed_count'] = t1[:failed]
    record['v1_total_count'] = t1[:total]
    record['v1_loc'] = count_loc(v1_dir, config[:exts])
    puts "  Tests: #{t1[:passed]}/#{t1[:total]} (#{t1[:success] ? 'PASS' : 'FAIL'})"
    puts "  LOC:   #{record['v1_loc']}"
  end

  # Phase 2: v2
  puts "\n--- Phase 2: v2 ---"
  FileUtils.cp_r(v1_dir, v2_dir)
  FileUtils.cp(File.join(UPSTREAM_DIR, 'SPEC-v2.txt'), v2_dir)
  FileUtils.cp(File.join(UPSTREAM_DIR, 'test-v2.sh'), v2_dir)

  if dry_run
    puts '  [DRY RUN]'
    record['v2_time'] = 0
  else
    v2 = run_claude(config[:v2_prompt] || GENERIC_V2_PROMPT,
                    dir: v2_dir, model: MODEL, log_path: File.join(LOGS_DIR, "#{slug}-v2.json"))
    record['v2_time'] = v2[:elapsed]
    record['v2_claude'] = v2[:claude_data]

    puts '  Running v2 tests...'
    t2 = run_tests('test-v2.sh', dir: v2_dir)
    record['v2_pass'] = t2[:success]
    record['v2_passed_count'] = t2[:passed]
    record['v2_failed_count'] = t2[:failed]
    record['v2_total_count'] = t2[:total]
    record['v2_loc'] = count_loc(v2_dir, config[:exts])
    puts "  Tests: #{t2[:passed]}/#{t2[:total]} (#{t2[:success] ? 'PASS' : 'FAIL'})"
    puts "  LOC:   #{record['v2_loc']}"
  end

  if dry_run
    puts '  [DRY RUN] record not appended'
  else
    File.open(RAW_JSONL, 'a') { |f| f.puts(JSON.generate(record)) }
    puts "  -> appended to #{RAW_JSONL}"
  end
  puts
end

puts '=' * 60
puts "Done. #{trials} trial(s) appended to #{RAW_JSONL}"
puts '=' * 60
