# frozen_string_literal: true
#
# ruby -Itest test/aggregate_edits_test.rb   (from research/benchmark/lang-bench)

require 'minitest/autorun'
require_relative '../aggregate_edits'

class WilsonTest < Minitest::Test
  def test_15_of_20
    lo, hi = EditsStats.wilson(15, 20)
    assert_in_delta 0.53, lo, 0.01
    assert_in_delta 0.88, hi, 0.01
  end

  def test_30_of_40
    lo, hi = EditsStats.wilson(30, 40)
    assert_in_delta 0.60, lo, 0.01
    assert_in_delta 0.86, hi, 0.01
  end

  def test_edges
    assert_equal [0.0, 0.0], EditsStats.wilson(0, 0)
    lo, hi = EditsStats.wilson(20, 20)
    assert_equal 1.0, hi
    assert lo > 0.8
    lo, = EditsStats.wilson(0, 20)
    assert_equal 0.0, lo
  end
end

class LedgerTest < Minitest::Test
  def rec(task, lang, survived, k: 3, usd: 0.5, status: nil)
    edits = (1..k).map do |i|
      s = i <= survived
      { 'k' => i, 'survived' => s, 'first_fail_mode' => (s ? nil : (i == survived + 1 ? 'hidden' : 'prefix')),
        'usd' => (i <= survived + 1 ? usd : 0.0) }
    end
    r = { 'task' => task, 'language' => lang, 'K' => k, 'edits' => edits }
    r['status'] = status if status
    r
  end

  def test_prefix_and_msr_edit
    records = [rec('t', 'almide', 3), rec('t', 'almide', 1), rec('t', 'almide', 3), rec('t', 'almide', 0)]
    rows, saturated = EditsStats.build_ledger(records)
    row = rows.first
    assert_equal 'measured', row['status']
    assert_equal 4, row['n']
    assert_in_delta 0.5, row['msr_seq'], 1e-9
    assert_in_delta (3 + 1 + 3 + 0) / 12.0, row['msr_edit'], 1e-9
    assert_includes %w[2:hidden 1:hidden], row['first_fail_mode']
    refute saturated
  end

  def test_status_rows
    records = [{ 'task' => 't', 'language' => 'zig', 'status' => 'harness-limitation', 'note' => 'zig not found' }]
    rows, = EditsStats.build_ledger(records, cells: [['t', 'zig'], ['t', 'go']])
    by = rows.to_h { |r| [r['language'], r['status']] }
    assert_equal 'harness-limitation', by['zig']
    assert_equal 'not-run', by['go']
  end

  def test_saturation
    records = (1..50).map { rec('t', 'rust', 3) } + (1..50).map { rec('t', 'almide', 3) }
    rows, saturated = EditsStats.build_ledger(records)
    assert saturated
    assert rows.all? { |r| r['status'] == 'inconclusive-saturated' }
    assert_match(/INCONCLUSIVE_BANK_SATURATED/, EditsStats.render(rows, saturated))
  end
end
