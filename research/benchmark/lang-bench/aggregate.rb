#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Aggregate raw/almide.jsonl → data.json (Almide entry).

require 'json'

SCRIPT_DIR = File.expand_path(__dir__)
RAW_JSONL  = File.join(SCRIPT_DIR, 'raw', 'almide.jsonl')
DATA_JSON  = File.join(SCRIPT_DIR, 'data.json')

abort "error: #{RAW_JSONL} not found" unless File.exist?(RAW_JSONL)
abort "error: #{DATA_JSON} not found" unless File.exist?(DATA_JSON)

records = File.readlines(RAW_JSONL).reject { |l| l.strip.empty? }.map { |l| JSON.parse(l) }
abort 'error: no records in raw/almide.jsonl' if records.empty?

def mean(xs)
  xs.sum.to_f / xs.length
end

def stddev(xs)
  return 0.0 if xs.length < 2

  m = mean(xs)
  Math.sqrt(xs.sum { |x| (x - m)**2 } / xs.length)
end

trials      = records.length
v1_pass     = records.count { |r| r['v1_pass'] }
v2_pass     = records.count { |r| r['v2_pass'] }
total_times = records.map { |r| (r['v1_time'] || 0) + (r['v2_time'] || 0) }
v2_locs     = records.map { |r| r['v2_loc'] || 0 }
costs       = records.map do |r|
  (r.dig('v1_claude', 'cost_usd') || 0.0) + (r.dig('v2_claude', 'cost_usd') || 0.0)
end

almide = {
  'name' => 'Almide',
  'model' => 'sonnet',
  'trials' => trials,
  'avg_total_time' => mean(total_times).round(1),
  'stddev_time' => stddev(total_times).round(1),
  'avg_v2_loc' => mean(v2_locs).round,
  'stddev_loc' => stddev(v2_locs).round,
  'v1_pass' => v1_pass,
  'v2_pass' => v2_pass,
  'avg_cost' => mean(costs).round(2),
}

data = JSON.parse(File.read(DATA_JSON))
data['languages'] ||= []
idx = data['languages'].index { |l| l['name'] == 'Almide' }
if idx
  data['languages'][idx] = almide
else
  data['languages'] << almide
end

File.write(DATA_JSON, "#{JSON.pretty_generate(data)}\n")

puts "Updated #{DATA_JSON}:"
puts JSON.pretty_generate(almide)
