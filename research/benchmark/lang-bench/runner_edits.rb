#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Modification-survival runner (#1963) — task-dir driven, cross-language.
#
# A task is a seed program per language plus an ORDERED edit sequence. Edit k
# runs in the model's own tree as it stands after edit k-1 (accumulating); an
# edit survives when
#
#   survive_k = compile ∧ visible ∧ hidden ∧ scope_clean
#
# where scope_clean means the files the model changed (git diff against the
# pre-edit snapshot, new files included) are all matched by scope.txt. The
# first failure fails every later horizon (prefix survival) and stops the
# sequence, so no money is spent on edits that cannot count.
#
# Task layout (Dojo tasks/xlang/<task>/, mirrored by tasks-example/xlang/):
#
#   seed/<lang>/                 the program before edit 1, with build.sh → ./app
#   edits/0k-<slug>/
#     prompt.md                  what the model is asked to do (shown)
#     scope.txt                  globs of files the edit may touch (shown)
#     visible.sh                 PASSED:/FAILED: suite the model may run (shown)
#     hidden.sh                  the oracle — never enters the model's tree
#     solution/<lang>/           reference patch, overlaid on the pre-edit tree
#     wrong/<lang>/              plausible wrong patch: passes visible, fails hidden
#
# An overlay directory may carry a `.remove` file (one path per line) for
# files the edit deletes. `bash build.sh` in the tree must leave an executable
# at ./app; visible.sh / hidden.sh run with the tree as cwd and drive ./app.
#
# Usage:
#   ruby runner_edits.rb --tasks DIR --dry-run                # validate every task, no model calls
#   ruby runner_edits.rb --tasks DIR --lang rust --trials 5   # measure one language
#   ruby runner_edits.rb --tasks DIR --task t1-ledger --lang almide --trials 5 --start 6
#
# Options: --lang L (default: every language present in seed/), --task T
# (default: every task), --trials N, --start K, --model M, --out DIR
# (default raw/xlang/), --almide-bin PATH (or ALMIDE_BIN) to pin the compiler.
#
# --dry-run is the bank gate: for every language present it proves the seed
# builds, the pre-edit tree FAILS each edit's visible and hidden suites (the
# edit is required), the solution builds and passes both, the wrong patch
# builds, passes visible and fails hidden, and every overlay stays inside
# scope.txt. A missing toolchain makes the row `harness-limitation`.

require 'json'
require 'fileutils'
require 'time'
require 'tmpdir'
require_relative 'lib/bench_common'

SCRIPT_DIR = File.expand_path(__dir__)
LOGS_DIR   = File.join(SCRIPT_DIR, '.logs', 'xlang')
WORK_DIR   = File.join(SCRIPT_DIR, '.work', 'xlang')
CHEATSHEET = File.expand_path(File.join(SCRIPT_DIR, '..', '..', '..', 'docs', 'CHEATSHEET.md'))

DEFAULT_MODEL = 'claude-sonnet-5'
BUILD_TIMEOUT = 300
TEST_TIMEOUT  = 300
TREE_IGNORE   = "app\napp.exe\n/target/\n/zig-out/\n/.zig-cache/\n/node_modules/\n*.o\n*.pdb\n"

# --- Args ------------------------------------------------------------------

opts = { tasks: nil, langs: [], tasks_filter: [], trials: 1, start: nil, dry_run: false,
         model: DEFAULT_MODEL, out: File.join(SCRIPT_DIR, 'raw', 'xlang'),
         almide_bin: ENV['ALMIDE_BIN'] }
i = 0
while i < ARGV.length
  case ARGV[i]
  when '--tasks'      then opts[:tasks] = File.expand_path(ARGV[i + 1]); i += 2
  when '--lang', '-l' then opts[:langs] << ARGV[i + 1]; i += 2
  when '--task'       then opts[:tasks_filter] << ARGV[i + 1]; i += 2
  when '--trials'     then opts[:trials] = ARGV[i + 1].to_i; i += 2
  when '--start'      then opts[:start] = ARGV[i + 1].to_i; i += 2
  when '--model'      then opts[:model] = ARGV[i + 1]; i += 2
  when '--out'        then opts[:out] = File.expand_path(ARGV[i + 1]); i += 2
  when '--almide-bin' then opts[:almide_bin] = File.expand_path(ARGV[i + 1]); i += 2
  when '--dry-run'    then opts[:dry_run] = true; i += 1
  when '--help', '-h' then puts File.read(__FILE__).lines.take_while { |l| l.start_with?('#') }.join; exit 0
  else
    abort "unknown arg: #{ARGV[i]}"
  end
end
abort 'error: --tasks DIR required' unless opts[:tasks] && File.directory?(opts[:tasks])

EXTRA_PATH = [opts[:almide_bin] && File.dirname(opts[:almide_bin]), bench_extra_path,
              File.join(Dir.home, '.local', 'bin')].compact.join(':')
MODEL_TAG  = opts[:model].delete('-').sub('claude', '')

# --- Task model ------------------------------------------------------------

Edit = Struct.new(:k, :slug, :dir) do
  def prompt   = File.join(dir, 'prompt.md')
  def scope    = File.join(dir, 'scope.txt')
  def visible  = File.join(dir, 'visible.sh')
  def hidden   = File.join(dir, 'hidden.sh')
  def solution(lang) = File.join(dir, 'solution', lang)
  def wrong(lang)    = File.join(dir, 'wrong', lang)

  def scope_globs
    File.readlines(scope).map(&:strip).reject { |l| l.empty? || l.start_with?('#') }
  end
end

Task = Struct.new(:name, :dir) do
  def seed(lang) = File.join(dir, 'seed', lang)
  def languages  = Dir.glob(File.join(dir, 'seed', '*')).select { |d| File.directory?(d) }.map { |d| File.basename(d) }.sort

  def edits
    Dir.glob(File.join(dir, 'edits', '*')).select { |d| File.directory?(d) }.sort.each_with_index.map do |d, idx|
      Edit.new(idx + 1, File.basename(d), d)
    end
  end
end

def load_tasks(root, filter)
  Dir.glob(File.join(root, '*')).select { |d| File.directory?(File.join(d, 'seed')) }.sort
     .map { |d| Task.new(File.basename(d), d) }
     .select { |t| filter.empty? || filter.include?(t.name) }
end

def in_scope?(path, globs)
  globs.any? { |g| File.fnmatch?(g, path, File::FNM_PATHNAME | File::FNM_EXTGLOB | File::FNM_DOTMATCH) }
end

# --- Tree operations -------------------------------------------------------

def sh(cmd, dir:, timeout: BUILD_TIMEOUT)
  run_cmd(cmd, dir: dir, timeout: timeout, extra_path: EXTRA_PATH)
end

def copy_tree(src, dst)
  FileUtils.rm_rf(dst)
  FileUtils.mkdir_p(dst)
  FileUtils.cp_r(Dir.glob(File.join(src, '{*,.[!.]*}')), dst)
end

# Files an overlay directory contributes (relative paths), plus removals.
def overlay_files(overlay)
  files = Dir.glob(File.join(overlay, '**', '{*,.[!.]*}'), File::FNM_DOTMATCH)
             .select { |f| File.file?(f) }
             .map { |f| f.sub("#{overlay}/", '') }
             .reject { |f| f == '.remove' }
  removes = File.exist?(File.join(overlay, '.remove')) ? File.readlines(File.join(overlay, '.remove')).map(&:strip).reject(&:empty?) : []
  [files, removes]
end

def apply_overlay(tree, overlay)
  files, removes = overlay_files(overlay)
  files.each do |rel|
    FileUtils.mkdir_p(File.dirname(File.join(tree, rel)))
    FileUtils.cp(File.join(overlay, rel), File.join(tree, rel))
  end
  removes.each { |rel| FileUtils.rm_rf(File.join(tree, rel)) }
  files + removes
end

DIAG_RE = /\b(E\d{3,4})\b/

def build(tree)
  FileUtils.rm_f(File.join(tree, 'app'))
  r = sh('bash build.sh', dir: tree)
  out = r[:stdout] + r[:stderr]
  ok = r[:success] && File.executable?(File.join(tree, 'app'))
  { ok: ok, diagnostic: (ok ? nil : out[DIAG_RE, 1]), output: out }
end

def suite(script, tree)
  r = run_tests(script, dir: tree, timeout: TEST_TIMEOUT, extra_path: EXTRA_PATH)
  r[:ok] = r[:success] && r[:total].positive? && r[:failed].zero?
  r
end

def toolchain(lang)
  cfg = BENCH_XLANG_LANGUAGES[lang]
  return [false, "#{lang}: not in the comparison set"] unless cfg

  probe = sh("command -v #{cfg[:probe]}", dir: nil)
  return [false, "#{cfg[:probe]} not found"] unless probe[:success]

  v = sh(cfg[:version_cmd], dir: nil)
  [true, (v[:stdout].strip.empty? ? v[:stderr].strip : v[:stdout].strip).lines.first&.strip || 'unknown']
end

# --- Dry run: the bank gate -------------------------------------------------

def dry_run(tasks)
  failures = 0
  rows = []
  tasks.each do |task|
    edits = task.edits
    if edits.empty?
      puts "#{task.name}: no edits/ — invalid"
      failures += 1
      next
    end
    edits.each do |e|
      %w[prompt scope visible hidden].each do |f|
        next if File.exist?(e.public_send(f))

        puts "#{task.name}/#{e.slug}: missing #{f}"
        failures += 1
      end
    end

    task.languages.each do |lang|
      ok, ver = toolchain(lang)
      unless ok
        rows << [task.name, lang, 'harness-limitation', ver]
        puts "#{task.name}/#{lang}: harness-limitation (#{ver})"
        next
      end
      errs = []
      Dir.mktmpdir("xlang-#{task.name}-#{lang}-") do |tmp|
        tree = File.join(tmp, 'tree')
        copy_tree(task.seed(lang), tree)
        errs << 'seed has no build.sh' unless File.exist?(File.join(tree, 'build.sh'))
        b = build(tree)
        errs << "seed does not build (#{b[:diagnostic] || 'no code'}): #{b[:output].lines.first&.strip}" unless b[:ok]
        break unless errs.empty?

        edits.each do |e|
          label = "edit #{e.k} (#{e.slug})"
          globs = e.scope_globs
          # (b) the edit is required: the pre-edit tree fails both suites
          errs << "#{label}: pre-edit tree already passes visible.sh" if suite(e.visible, tree)[:ok]
          errs << "#{label}: pre-edit tree already passes hidden.sh" if suite(e.hidden, tree)[:ok]

          # (c) a correct patch exists, inside scope
          sol = e.solution(lang)
          unless File.directory?(sol)
            errs << "#{label}: missing solution/#{lang}"
            break
          end
          sol_tree = File.join(tmp, "sol#{e.k}")
          copy_tree(tree, sol_tree)
          touched = apply_overlay(sol_tree, sol)
          out_of_scope = touched.reject { |p| in_scope?(p, globs) }
          errs << "#{label}: solution touches files outside scope.txt: #{out_of_scope.join(', ')}" unless out_of_scope.empty?
          b = build(sol_tree)
          if b[:ok]
            v = suite(e.visible, sol_tree)
            h = suite(e.hidden, sol_tree)
            errs << "#{label}: solution fails visible.sh (#{v[:passed]}/#{v[:total]})" unless v[:ok]
            errs << "#{label}: solution fails hidden.sh (#{h[:passed]}/#{h[:total]})" unless h[:ok]
          else
            errs << "#{label}: solution does not build (#{b[:diagnostic] || 'no code'}): #{b[:output].lines.first&.strip}"
          end

          # (d) the bank discriminates
          wrong = e.wrong(lang)
          if File.directory?(wrong)
            wrong_tree = File.join(tmp, "wrong#{e.k}")
            copy_tree(tree, wrong_tree)
            touched = apply_overlay(wrong_tree, wrong)
            out_of_scope = touched.reject { |p| in_scope?(p, globs) }
            errs << "#{label}: wrong patch touches files outside scope.txt: #{out_of_scope.join(', ')}" unless out_of_scope.empty?
            b = build(wrong_tree)
            if b[:ok]
              errs << "#{label}: wrong patch does not pass visible.sh — not plausible" unless suite(e.visible, wrong_tree)[:ok]
              errs << "#{label}: wrong patch passes hidden.sh — the oracle does not discriminate" if suite(e.hidden, wrong_tree)[:ok]
            else
              errs << "#{label}: wrong patch does not build (#{b[:diagnostic] || 'no code'})"
            end
          else
            errs << "#{label}: missing wrong/#{lang}"
          end

          tree = sol_tree # accumulate: edit k+1 starts from the reference solution of edit k
        end
      end
      status = errs.empty? ? 'ok' : 'invalid'
      rows << [task.name, lang, status, ver]
      puts "#{task.name}/#{lang}: #{status} (#{ver}; #{edits.length} edits)"
      errs.each { |m| puts "    - #{m}" }
      failures += errs.length
    end
  end
  puts
  puts format('%-16s %-12s %-20s %s', 'task', 'language', 'status', 'toolchain')
  rows.each { |r| puts format('%-16s %-12s %-20s %s', *r) }
  puts
  puts(failures.zero? ? 'dry-run OK' : "dry-run FAILED (#{failures} problem(s))")
  failures.zero?
end

# --- Live loop ---------------------------------------------------------------

def git(tree, args)
  sh("git #{args}", dir: tree, timeout: 60)
end

def snapshot(tree, msg)
  git(tree, 'add -A')
  git(tree, "-c user.name=xlang -c user.email=xlang@localhost commit -q --allow-empty -m #{Shellwords.escape(msg)}")
end

def changed_files(tree)
  git(tree, 'add -A')
  git(tree, 'diff --cached --name-only HEAD')[:stdout].lines.map(&:strip).reject(&:empty?)
end

def model_prompt(edit, lang)
  cfg = BENCH_XLANG_LANGUAGES[lang]
  lines = []
  lines << "You are modifying an existing #{cfg[:display]} program in the current directory."
  lines << ''
  lines << File.read(edit.prompt).strip
  lines << ''
  lines << 'Rules:'
  lines << '- Build with: bash build.sh   (it must leave an executable at ./app)'
  lines << '- Verify with: bash visible.sh   (a hidden suite with the same interface is scored afterwards; do not edit visible.sh)'
  lines << "- Only files matching these patterns may change: #{edit.scope_globs.join(' ')}"
  lines << '- Almide is a new language. Read CHEATSHEET.md for the complete language reference.' if cfg[:copy_cheatsheet]
  lines.join("\n")
end

def run_sequence(task, lang, trial, opts, toolchain_ver, claude_ver)
  edits = task.edits
  slug  = "#{task.name}-#{lang}-#{MODEL_TAG}-#{trial}"
  tree  = File.join(WORK_DIR, slug)
  copy_tree(task.seed(lang), tree)
  File.write(File.join(tree, '.gitignore'), TREE_IGNORE)
  FileUtils.cp(CHEATSHEET, tree) if BENCH_XLANG_LANGUAGES[lang][:copy_cheatsheet]
  git(tree, 'init -q')
  snapshot(tree, 'seed')

  record = {
    'language' => lang, 'task' => task.name, 'trial' => trial, 'model' => opts[:model],
    'status' => 'measured', 'timestamp' => Time.now.iso8601,
    'toolchain_version' => toolchain_ver, 'claude_version' => claude_ver,
    'K' => edits.length, 'edits' => [],
  }

  alive = true
  edits.each do |e|
    entry = { 'k' => e.k, 'slug' => e.slug, 'attempted' => false, 'survived' => false, 'first_fail_mode' => 'prefix',
              'usd' => 0.0, 'seconds' => 0.0 }
    unless alive
      record['edits'] << entry
      next
    end

    puts "  edit #{e.k}/#{edits.length}: #{e.slug}"
    FileUtils.cp(e.visible, File.join(tree, 'visible.sh'))
    snapshot(tree, "pre-edit-#{e.k}")

    c = run_claude(model_prompt(e, lang), dir: tree, model: opts[:model],
                   log_path: File.join(LOGS_DIR, "#{slug}-edit#{e.k}.json"), extra_path: EXTRA_PATH)
    entry['attempted'] = true
    entry['seconds']   = c[:elapsed]
    entry['usd']       = c.dig(:claude_data, 'cost_usd') || 0.0
    entry['claude']    = c[:claude_data]

    changed = changed_files(tree)
    entry['changed_files'] = changed
    out_of_scope = changed.reject { |p| in_scope?(p, e.scope_globs) }
    entry['out_of_scope'] = out_of_scope

    b = build(tree)
    entry['compile']    = b[:ok]
    entry['diagnostic'] = b[:diagnostic]
    if b[:ok]
      v = suite(e.visible, tree)
      h = suite(e.hidden, tree)
      entry['visible'] = { 'passed' => v[:passed], 'failed' => v[:failed], 'ok' => v[:ok] }
      entry['hidden']  = { 'passed' => h[:passed], 'failed' => h[:failed], 'ok' => h[:ok] }
    end
    entry['scope_clean'] = out_of_scope.empty?
    entry['loc'] = count_loc(tree, BENCH_XLANG_LANGUAGES[lang][:exts], script_name: nil)

    mode = if !b[:ok] then 'compile'
           elsif !entry.dig('visible', 'ok') then 'visible'
           elsif !entry.dig('hidden', 'ok') then 'hidden'
           elsif !entry['scope_clean'] then 'scope'
           end
    entry['survived'] = mode.nil?
    entry['first_fail_mode'] = mode
    puts "    #{mode ? "FAIL (#{mode})" : 'survived'}  $#{format('%.2f', entry['usd'])}  #{entry['seconds']}s"
    snapshot(tree, "post-edit-#{e.k}")
    record['edits'] << entry
    alive = false if mode
  end
  record['survived_k'] = record['edits'].count { |x| x['survived'] }
  record
end

# --- Main --------------------------------------------------------------------

tasks = load_tasks(opts[:tasks], opts[:tasks_filter])
abort "error: no tasks under #{opts[:tasks]}" if tasks.empty?

if opts[:dry_run]
  exit(dry_run(tasks) ? 0 : 1)
end

abort 'error: claude CLI not found in PATH' unless system('command -v claude >/dev/null 2>&1')
abort "error: CHEATSHEET.md not found at #{CHEATSHEET}" unless File.exist?(CHEATSHEET)
FileUtils.mkdir_p([opts[:out], WORK_DIR, LOGS_DIR])
claude_ver = `claude --version 2>/dev/null`.strip

langs = opts[:langs].empty? ? tasks.flat_map(&:languages).uniq.sort : opts[:langs]
langs.each do |lang|
  raw = File.join(opts[:out], "#{lang}-#{MODEL_TAG}.jsonl")
  existing = File.exist?(raw) ? File.readlines(raw).map { |l| JSON.parse(l) } : []
  ok, ver = toolchain(lang)
  puts '=' * 60
  puts "xlang runner — #{lang} (#{opts[:model]})  toolchain: #{ver}"
  puts '=' * 60
  tasks.each do |task|
    next unless task.languages.include?(lang)

    start = opts[:start] || ((existing.select { |r| r['task'] == task.name }.map { |r| r['trial'] }.max || 0) + 1)
    unless ok
      rec = { 'language' => lang, 'task' => task.name, 'trial' => start, 'model' => opts[:model],
              'status' => 'harness-limitation', 'note' => ver, 'timestamp' => Time.now.iso8601, 'K' => task.edits.length, 'edits' => [] }
      File.open(raw, 'a') { |f| f.puts(JSON.generate(rec)) }
      puts "#{task.name}: harness-limitation (#{ver}) — recorded"
      next
    end
    opts[:trials].times do |idx|
      trial = start + idx
      puts "--- #{task.name} trial #{trial} (#{idx + 1}/#{opts[:trials]}) ---"
      rec = run_sequence(task, lang, trial, opts, ver, claude_ver)
      File.open(raw, 'a') { |f| f.puts(JSON.generate(rec)) }
      puts "  -> #{rec['survived_k']}/#{rec['K']} survived, appended to #{raw}"
    end
  end
end
