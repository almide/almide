# The same in Python: a dict lookup that can be absent, used directly.
def get(xs, i):
    return xs[i] if 0 <= i < len(xs) else None
def main():
    xs = [10, 20, 30]
    a = get(xs, 0)
    b = get(xs, 5)        # absent -> None
    print(a + b)          # None mishandled
main()
