#!/bin/bash
# Verb Benchmark: measure LLM accuracy on Almide function name prediction
# Usage: ./tools/verb-benchmark/run.sh [model]
# Default model: haiku

set -euo pipefail

MODEL="${1:-haiku}"
CASES="tools/verb-benchmark/cases.json"
RESULTS="tools/verb-benchmark/results-${MODEL}.json"

SYSTEM_PROMPT='You are predicting Almide stdlib function names. Almide is a programming language with modules like list, map, string, int, float, result, option, json, fs, http, regex.

Rules:
- Answer with ONLY the function name (e.g., "map", "filter", "get"). No module prefix, no parentheses, no explanation.
- Use snake_case.
- One word or two words joined by underscore.'

TOTAL=$(python3 -c "import json; print(len(json.load(open('$CASES'))))")
CORRECT=0
PARTIAL=0
WRONG=0

echo "Running verb benchmark with model: $MODEL ($TOTAL cases)"
echo "=========================================="

python3 -c "
import json, subprocess, sys

cases = json.load(open('$CASES'))
results = []
correct = partial = wrong = 0

for i, case in enumerate(cases):
    desc = case['desc']
    module = case['module']
    expected = case['expected']
    alts = case.get('alt', [])

    prompt = f'Almide の {module} モジュールで「{desc}」関数の名前は？関数名だけ答えて。'

    try:
        result = subprocess.run(
            ['claude', '-p', '--model', '$MODEL', prompt],
            capture_output=True, text=True, timeout=30,
            env={**__import__('os').environ, 'CLAUDE_CODE_ENTRYPOINT': 'cli'}
        )
        answer = result.stdout.strip().lower().replace('()', '').replace('\"', '').replace(\"'\", '').split('.')[-1].split('(')[0].strip()
    except Exception as e:
        answer = f'ERROR: {e}'

    if answer == expected:
        status = '✅'
        correct += 1
    elif answer in alts:
        status = '🔶'
        partial += 1
    else:
        status = '❌'
        wrong += 1

    print(f'  {status} {module}.{expected:15s} got: {answer:15s} ({desc})')
    results.append({**case, 'answer': answer, 'status': 'correct' if answer == expected else ('alt' if answer in alts else 'wrong')})

print()
print(f'Results: {correct} correct, {partial} alt-correct, {wrong} wrong / {len(cases)} total')
print(f'Accuracy: {100*correct/len(cases):.1f}% exact, {100*(correct+partial)/len(cases):.1f}% with alts')

with open('$RESULTS', 'w') as f:
    json.dump({'model': '$MODEL', 'total': len(cases), 'correct': correct, 'partial': partial, 'wrong': wrong,
               'accuracy_exact': round(100*correct/len(cases), 1), 'accuracy_with_alts': round(100*(correct+partial)/len(cases), 1),
               'cases': results}, f, indent=2, ensure_ascii=False)
print(f'Results saved to: $RESULTS')
"
