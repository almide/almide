#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Aggregate raw/<lang>-sonnet5.jsonl records into per-language snapshot stats.
#
# Usage:
#   ruby aggregate_multi.rb            # print table
#   ruby aggregate_multi.rb --json     # emit snapshot JSON to stdout

require 'json'

SCRIPT_DIR = File.expand_path(__dir__)
RAW_DIR    = File.join(SCRIPT_DIR, 'raw')

LANG_DISPLAY = {
  'almide' => 'Almide', 'gleam' => 'Gleam', 'moonbit' => 'MoonBit',
  'rust' => 'Rust', 'typescript' => 'TypeScript',
}.freeze

emit_json = ARGV.include?('--json')

def mean(a) = a.empty? ? 0.0 : a.sum.to_f / a.length

def stddev(a)
  return 0.0 if a.length < 2

  m = mean(a)
  Math.sqrt(a.sum { |x| (x - m)**2 } / (a.length - 1))
end

snapshot = []

LANG_DISPLAY.each do |lang, display|
  path = File.join(RAW_DIR, "#{lang}-sonnet5.jsonl")
  next unless File.exist?(path)

  records = File.readlines(path).map { |l| JSON.parse(l) }
  next if records.empty?

  total_times = records.map { |r| (r['v1_time'] || 0) + (r['v2_time'] || 0) }
  v2_locs     = records.select { |r| r['v2_pass'] }.map { |r| r['v2_loc'] || 0 }
  costs       = records.map do |r|
    (r.dig('v1_claude', 'cost_usd') || 0) + (r.dig('v2_claude', 'cost_usd') || 0)
  end

  snapshot << {
    'name' => display,
    'model' => records.first['model'],
    'toolchain' => records.first['toolchain_version'],
    'trials' => records.length,
    'v1_pass' => records.count { |r| r['v1_pass'] },
    'v2_pass' => records.count { |r| r['v2_pass'] },
    'avg_total_time' => mean(total_times).round(1),
    'stddev_time' => stddev(total_times).round(1),
    'avg_v2_loc' => mean(v2_locs).round,
    'stddev_loc' => stddev(v2_locs).round,
    'avg_cost' => mean(costs).round(2),
  }
end

if emit_json
  puts JSON.pretty_generate(snapshot)
else
  puts format('%-12s %-7s %8s %8s %12s %10s %8s',
              'lang', 'trials', 'v1_pass', 'v2_pass', 'avg_time(s)', 'avg_loc', 'avg_$')
  snapshot.each do |s|
    puts format('%-12s %-7d %8s %8s %12.1f %10d %8.2f',
                s['name'], s['trials'],
                "#{s['v1_pass']}/#{s['trials']}", "#{s['v2_pass']}/#{s['trials']}",
                s['avg_total_time'], s['avg_v2_loc'], s['avg_cost'])
  end
end
