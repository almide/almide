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
#   - per-language toolchain in PATH (almide / gleam / moon / rustc / tsx)

require 'json'
require 'fileutils'
require 'time'
require 'open3'
require 'timeout'
require 'shellwords'

SCRIPT_DIR   = File.expand_path(__dir__)
UPSTREAM_DIR = File.join(SCRIPT_DIR, 'upstream')
RAW_DIR      = File.join(SCRIPT_DIR, 'raw')
WORK_DIR     = File.join(SCRIPT_DIR, '.work')
LOGS_DIR     = File.join(SCRIPT_DIR, '.logs')
CHEATSHEET   = File.expand_path(File.join(SCRIPT_DIR, '..', '..', '..', 'docs', 'CHEATSHEET.md'))
NPM_BIN      = File.join(SCRIPT_DIR, '.npm-prefix', 'node_modules', '.bin')

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
}.freeze

EXCLUDE_DIR_FRAGMENTS = %w[/node_modules/ /target/ /build/ /_build/ /.minigit/ /deps/].freeze

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

def extra_path
  "#{File.join(Dir.home, '.moon', 'bin')}:#{NPM_BIN}:/opt/homebrew/bin"
end

def run_cmd(cmd, dir: nil, timeout: 600)
  opts = {}
  opts[:chdir] = dir if dir
  stdin, stdout, stderr, wait_thr = Open3.popen3("export PATH=#{extra_path}:$PATH && #{cmd}", **opts)
  stdin.close
  stdout.set_encoding('UTF-8')
  stderr.set_encoding('UTF-8')
  out = err = +''
  begin
    Timeout.timeout(timeout) do
      out = stdout.read
      err = stderr.read
    end
  rescue Timeout::Error
    (Process.kill('TERM', wait_thr.pid) rescue nil)
    out = (stdout.read rescue '')
    err = "Timeout after #{timeout}s"
  end
  stdout.close
  stderr.close
  status = wait_thr.value
  { stdout: out, stderr: err, exit_code: status.exitstatus, success: status.success? }
end

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

# --- Helpers ---------------------------------------------------------------

def parse_claude_json(raw)
  raw = raw.dup.force_encoding('UTF-8')
  events = JSON.parse(raw.strip)
  events = [events] unless events.is_a?(Array)
  result = events.reverse.find { |e| e.is_a?(Hash) && e['type'] == 'result' }
  return nil unless result

  usage = result['usage'] || {}
  {
    'input_tokens' => usage['input_tokens'] || 0,
    'output_tokens' => usage['output_tokens'] || 0,
    'cache_creation_tokens' => usage['cache_creation_input_tokens'] || 0,
    'cache_read_tokens' => usage['cache_read_input_tokens'] || 0,
    'cost_usd' => result['total_cost_usd'] || 0.0,
    'num_turns' => result['num_turns'] || 0,
    'duration_ms' => result['duration_ms'] || 0,
  }
rescue JSON::ParserError => e
  warn "warn: failed to parse Claude JSON: #{e.message}"
  nil
end

def run_claude(prompt, dir:, log_path: nil)
  cmd = "unset CLAUDECODE && claude -p #{Shellwords.escape(prompt)} " \
        "--dangerously-skip-permissions --output-format json --model #{MODEL}"
  puts "  Running Claude (#{MODEL})..."
  t0 = Time.now
  result = run_cmd(cmd, dir: dir, timeout: 1800)
  elapsed = (Time.now - t0).round(1)
  if log_path
    FileUtils.mkdir_p(File.dirname(log_path))
    File.write(log_path, result[:stdout])
  end
  {
    success: result[:success],
    elapsed: elapsed,
    claude_data: parse_claude_json(result[:stdout]),
  }
end

def run_tests(script, dir:)
  result = run_cmd("bash #{Shellwords.escape(script)}", dir: dir, timeout: 300)
  output = result[:stdout] + result[:stderr]
  passed = output[/PASSED:\s*(\d+)/, 1]&.to_i || 0
  failed = output[/FAILED:\s*(\d+)/, 1]&.to_i || 0
  { success: result[:success], passed: passed, failed: failed, total: passed + failed }
end

def count_loc(dir, exts)
  files = exts.flat_map { |e| Dir.glob(File.join(dir, '**', "*.#{e}")) }
  files.reject! { |f| EXCLUDE_DIR_FRAGMENTS.any? { |frag| f.include?(frag) } }

  # For scripting languages the executable `minigit` may BE the source
  minigit = File.join(dir, 'minigit')
  if File.exist?(minigit) && !files.include?(minigit)
    begin
      content = File.read(minigit, encoding: 'UTF-8')
      files << minigit if content.valid_encoding?
    rescue StandardError
      # skip binary
    end
  end

  files.sum do |f|
    File.readlines(f).count { |l| !l.strip.empty? }
  rescue StandardError
    0
  end
end

# --- Warmup ----------------------------------------------------------------

unless dry_run
  puts '--- Warmup ---'
  warmup = File.join(WORK_DIR, ".warmup-#{lang}")
  FileUtils.rm_rf(warmup)
  FileUtils.mkdir_p(warmup)
  w = run_claude('Respond with just the word OK.', dir: warmup)
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
                    dir: v1_dir, log_path: File.join(LOGS_DIR, "#{slug}-v1.json"))
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
                    dir: v2_dir, log_path: File.join(LOGS_DIR, "#{slug}-v2.json"))
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
