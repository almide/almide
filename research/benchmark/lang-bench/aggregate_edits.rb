#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Aggregate runner_edits.rb records (raw/xlang/<lang>-<model>.jsonl) into the
# modification-survival ledger of #1963.
#
# Per (task, language) cell:
#   n               sequences (trials) recorded
#   msr_seq         fraction of sequences that survived ALL K edits, Wilson 95 % CI
#   msr_edit        mean over sequences of k*/K, k* = edits survived before the first failure
#   first_fail_mode the most common (edit, mode) at which failed sequences died
#   usd/seq         mean model spend per sequence
#   status          measured | not-run | harness-limitation | inconclusive-saturated
#
# INCONCLUSIVE_BANK_SATURATED is printed when every measured cell is at or
# above the saturation ceiling (98 %): the bank is then a regression suite,
# not ranking evidence (almide-dojo#3).
#
# Usage:
#   ruby aggregate_edits.rb                          # ledger from raw/xlang/
#   ruby aggregate_edits.rb --raw DIR --tasks DIR    # DIR/xlang tasks enumerate not-run cells
#   ruby aggregate_edits.rb --json

require 'json'

module EditsStats
  SATURATION = 0.98
  Z95        = 1.959964

  module_function

  # Wilson score interval for k successes in n trials.
  def wilson(k, n, z: Z95)
    return [0.0, 0.0] if n <= 0

    p = k.to_f / n
    denom  = 1 + z * z / n
    centre = (p + z * z / (2 * n)) / denom
    half   = z * Math.sqrt(p * (1 - p) / n + z * z / (4.0 * n * n)) / denom
    [[centre - half, 0.0].max, [centre + half, 1.0].min]
  end

  def mean(a) = a.empty? ? 0.0 : a.sum.to_f / a.length

  # Edits survived before the first failure (prefix survival).
  def survived_prefix(record)
    edits = record['edits'] || []
    k = 0
    edits.each do |e|
      break unless e['survived']

      k += 1
    end
    k
  end

  def first_failure(record)
    e = (record['edits'] || []).find { |x| !x['survived'] }
    return nil unless e

    "#{e['k']}:#{e['first_fail_mode']}"
  end

  # One ledger row for a cell of records sharing (task, language).
  def summarize_cell(task, language, records)
    limitations = records.select { |r| r['status'] == 'harness-limitation' }
    measured    = records.reject { |r| r['status'] == 'harness-limitation' }
    if measured.empty?
      status = limitations.empty? ? 'not-run' : 'harness-limitation'
      return { 'task' => task, 'language' => language, 'status' => status, 'n' => 0,
               'msr_seq' => nil, 'ci95' => nil, 'msr_edit' => nil, 'first_fail_mode' => nil,
               'usd_per_seq' => nil, 'note' => limitations.first&.dig('note') }
    end

    n   = measured.length
    ks  = measured.map { |r| [r['K'] || (r['edits'] || []).length, 1].max }
    ok  = measured.count { |r| survived_prefix(r) == (r['K'] || (r['edits'] || []).length) }
    lo, hi = wilson(ok, n)
    msr_edit = mean(measured.each_with_index.map { |r, i| survived_prefix(r).to_f / ks[i] })
    modes = measured.filter_map { |r| first_failure(r) }.tally
    usd = mean(measured.map { |r| (r['edits'] || []).sum { |e| e['usd'] || 0.0 } })
    {
      'task' => task, 'language' => language, 'status' => 'measured', 'n' => n,
      'seq_ok' => ok, 'K' => ks.max,
      'msr_seq' => ok.to_f / n, 'ci95' => [lo, hi], 'msr_edit' => msr_edit,
      'first_fail_mode' => modes.max_by { |_, c| c }&.first,
      'usd_per_seq' => usd,
    }
  end

  # rows: from summarize_cell. Marks a task's measured rows inconclusive when
  # every measured language of that task is at the ceiling; returns
  # [rows, saturated_globally].
  def apply_saturation(rows)
    by_task = rows.group_by { |r| r['task'] }
    by_task.each_value do |trs|
      m = trs.select { |r| r['status'] == 'measured' }
      next if m.empty? || m.any? { |r| r['msr_seq'] < SATURATION }

      m.each { |r| r['status'] = 'inconclusive-saturated' }
    end
    measured = rows.select { |r| %w[measured inconclusive-saturated].include?(r['status']) }
    saturated = !measured.empty? && measured.all? { |r| r['msr_seq'] >= SATURATION }
    [rows, saturated]
  end

  def build_ledger(records, cells: nil)
    grouped = records.group_by { |r| [r['task'], r['language']] }
    keys = grouped.keys
    keys |= cells if cells
    rows = keys.sort.map { |task, lang| summarize_cell(task, lang, grouped[[task, lang]] || []) }
    apply_saturation(rows)
  end

  def fmt_pct(x) = x.nil? ? '-' : format('%.2f', x)

  def render(rows, saturated)
    out = +''
    out << "| task | language | status | n | msr_seq | ci95 | msr_edit | first_fail_mode | usd/seq |\n"
    out << "|---|---|---|---|---|---|---|---|---|\n"
    rows.each do |r|
      ci = r['ci95'] ? "#{fmt_pct(r['ci95'][0])}–#{fmt_pct(r['ci95'][1])}" : '-'
      usd = r['usd_per_seq'] ? format('$%.2f', r['usd_per_seq']) : '-'
      out << "| #{r['task']} | #{r['language']} | #{r['status']} | #{r['n']} | #{fmt_pct(r['msr_seq'])} | #{ci} | " \
             "#{fmt_pct(r['msr_edit'])} | #{r['first_fail_mode'] || '-'} | #{usd} |\n"
    end
    out << "\nINCONCLUSIVE_BANK_SATURATED: every measured cell is at or above #{(SATURATION * 100).round} %; " \
           "the bank ranks nothing (regression suite only)\n" if saturated
    out
  end
end

if __FILE__ == $PROGRAM_NAME
  script_dir = File.expand_path(__dir__)
  raw_dir    = File.join(script_dir, 'raw', 'xlang')
  tasks_dir  = nil
  emit_json  = false
  i = 0
  while i < ARGV.length
    case ARGV[i]
    when '--raw'   then raw_dir = File.expand_path(ARGV[i + 1]); i += 2
    when '--tasks' then tasks_dir = File.expand_path(ARGV[i + 1]); i += 2
    when '--json'  then emit_json = true; i += 1
    else
      warn "unknown arg: #{ARGV[i]}"; i += 1
    end
  end

  records = Dir.glob(File.join(raw_dir, '*.jsonl')).sort.flat_map do |path|
    File.readlines(path).map(&:strip).reject(&:empty?).map { |l| JSON.parse(l) }
  end

  cells = nil
  if tasks_dir
    langs = %w[almide rust go typescript zig]
    cells = Dir.glob(File.join(tasks_dir, '*', 'seed', '*')).select { |d| File.directory?(d) }.map do |d|
      [File.basename(File.dirname(File.dirname(d))), File.basename(d)]
    end
    cells = cells.select { |_, l| langs.include?(l) }
  end

  rows, saturated = EditsStats.build_ledger(records, cells: cells)
  if emit_json
    puts JSON.pretty_generate({ 'saturated' => saturated, 'rows' => rows })
  else
    puts EditsStats.render(rows, saturated)
  end
end
