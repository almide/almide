#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Almide lang-bench runner.
#
# Runs the mame/ai-coding-lang-bench minigit task against Almide,
# using Claude Code CLI (Sonnet 4.6), and appends results to raw/almide.jsonl.
#
# Usage:
#   ruby runner.rb --trials 10
#   ruby runner.rb --trials 1 --dry-run
#
# Requirements:
#   - git submodule initialized (upstream/)
#   - claude CLI in PATH
#   - almide binary in PATH

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
RAW_JSONL    = File.join(RAW_DIR, 'almide.jsonl')
CHEATSHEET   = File.expand_path(File.join(SCRIPT_DIR, '..', '..', '..', 'docs', 'CHEATSHEET.md'))

# --- Arg parsing -----------------------------------------------------------

trials = 1
dry_run = false
i = 0
while i < ARGV.length
  case ARGV[i]
  when '--trials', '-t' then trials = ARGV[i + 1].to_i; i += 2
  when '--dry-run'      then dry_run = true; i += 1
  else
    warn "unknown arg: #{ARGV[i]}"; i += 1
  end
end

# --- Preflight -------------------------------------------------------------

unless File.exist?(File.join(UPSTREAM_DIR, 'SPEC-v1.txt'))
  abort "error: upstream submodule not initialized. Run:\n  git submodule update --init #{File.join('research', 'benchmark', 'lang-bench', 'upstream')}"
end

%w[claude almide ruby].each do |cmd|
  unless system("command -v #{cmd} >/dev/null 2>&1")
    abort "error: #{cmd} not found in PATH"
  end
end

abort "error: CHEATSHEET.md not found at #{CHEATSHEET}" unless File.exist?(CHEATSHEET)

FileUtils.mkdir_p([RAW_DIR, WORK_DIR, LOGS_DIR])

existing = File.exist?(RAW_JSONL) ? File.readlines(RAW_JSONL).map { |l| JSON.parse(l) } : []
start_trial = (existing.map { |r| r['trial'] }.max || 0) + 1

upstream_rev = `cd #{Shellwords.escape(UPSTREAM_DIR)} && git rev-parse --short HEAD`.strip
almide_ver = `almide --version 2>&1`.strip

puts '=' * 60
puts 'Almide lang-bench runner'
puts '=' * 60
puts "Upstream commit: #{upstream_rev}"
puts "almide version:  #{almide_ver}"
puts "Existing trials: #{existing.length}"
puts "Start trial:     #{start_trial}"
puts "Trials to run:   #{trials}"
puts "Dry run:         #{dry_run}"
puts

# --- Helpers ---------------------------------------------------------------

def run_cmd(cmd, dir: nil, timeout: 600)
  opts = {}
  opts[:chdir] = dir if dir
  stdin, stdout, stderr, wait_thr = Open3.popen3(cmd, **opts)
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
        '--dangerously-skip-permissions --output-format json --model sonnet'
  puts '  Running Claude (Sonnet)...'
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
  result = run_cmd("bash #{Shellwords.escape(script)}", dir: dir, timeout: 120)
  output = result[:stdout] + result[:stderr]
  passed = output[/PASSED:\s*(\d+)/, 1]&.to_i || 0
  failed = output[/FAILED:\s*(\d+)/, 1]&.to_i || 0
  { success: result[:success], passed: passed, failed: failed, total: passed + failed }
end

def count_loc(dir)
  files = Dir.glob(File.join(dir, '**', '*.almd'))
  minigit = File.join(dir, 'minigit')
  if File.exist?(minigit)
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

V1_PROMPT = <<~PROMPT.strip
  Implement minigit as described in SPEC-v1.txt using Almide.
  The executable must be named 'minigit' and be runnable as ./minigit.
  Verify your implementation passes all tests by running: bash test-v1.sh
  Almide is a new language. Read CHEATSHEET.md for the complete language reference.
  Build with: almide build main.almd -o minigit
PROMPT

V2_PROMPT = <<~PROMPT.strip
  Read SPEC-v2.txt and extend the existing minigit implementation
  with checkout and reset commands.
  Verify your implementation passes all tests by running: bash test-v2.sh
  Almide is a new language. Read CHEATSHEET.md for the complete language reference.
  Build with: almide build main.almd -o minigit
PROMPT

# --- Warmup ----------------------------------------------------------------

unless dry_run
  puts '--- Warmup ---'
  warmup = File.join(WORK_DIR, '.warmup')
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
  puts "Trial #{trial} (#{idx + 1}/#{trials})"
  puts '=' * 60

  v1_dir = File.join(WORK_DIR, "minigit-almide-#{trial}-v1")
  v2_dir = File.join(WORK_DIR, "minigit-almide-#{trial}-v2")
  FileUtils.rm_rf(v1_dir)
  FileUtils.rm_rf(v2_dir)
  FileUtils.mkdir_p(v1_dir)

  record = {
    'language' => 'almide',
    'model' => 'sonnet',
    'trial' => trial,
    'timestamp' => Time.now.iso8601,
    'upstream_commit' => upstream_rev,
    'almide_version' => almide_ver,
  }

  # Phase 1: v1
  puts "\n--- Phase 1: v1 ---"
  FileUtils.cp(File.join(UPSTREAM_DIR, 'SPEC-v1.txt'), v1_dir)
  FileUtils.cp(File.join(UPSTREAM_DIR, 'test-v1.sh'), v1_dir)
  FileUtils.cp(CHEATSHEET, v1_dir)

  if dry_run
    puts '  [DRY RUN]'
    record['v1_time'] = 0
  else
    v1_log = File.join(LOGS_DIR, "minigit-almide-#{trial}-v1.json")
    v1 = run_claude(V1_PROMPT, dir: v1_dir, log_path: v1_log)
    record['v1_time'] = v1[:elapsed]
    record['v1_claude'] = v1[:claude_data]

    puts '  Running v1 tests...'
    t1 = run_tests('test-v1.sh', dir: v1_dir)
    record['v1_pass'] = t1[:success]
    record['v1_passed_count'] = t1[:passed]
    record['v1_failed_count'] = t1[:failed]
    record['v1_total_count'] = t1[:total]
    record['v1_loc'] = count_loc(v1_dir)
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
    v2_log = File.join(LOGS_DIR, "minigit-almide-#{trial}-v2.json")
    v2 = run_claude(V2_PROMPT, dir: v2_dir, log_path: v2_log)
    record['v2_time'] = v2[:elapsed]
    record['v2_claude'] = v2[:claude_data]

    puts '  Running v2 tests...'
    t2 = run_tests('test-v2.sh', dir: v2_dir)
    record['v2_pass'] = t2[:success]
    record['v2_passed_count'] = t2[:passed]
    record['v2_failed_count'] = t2[:failed]
    record['v2_total_count'] = t2[:total]
    record['v2_loc'] = count_loc(v2_dir)
    puts "  Tests: #{t2[:passed]}/#{t2[:total]} (#{t2[:success] ? 'PASS' : 'FAIL'})"
    puts "  LOC:   #{record['v2_loc']}"
  end

  File.open(RAW_JSONL, 'a') { |f| f.puts(JSON.generate(record)) }
  puts "  -> appended to #{RAW_JSONL}"
  puts
end

puts '=' * 60
puts "Done. #{trials} trial(s) appended to #{RAW_JSONL}"
puts "Next: ruby #{File.join(SCRIPT_DIR, 'aggregate.rb')}"
puts '=' * 60
