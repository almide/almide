## Laws
A `law` block states a property that must hold for EVERY input:

    law "reverse twice is identity" for (xs: List[Int]) {
      list.reverse(list.reverse(xs)) == xs
    }

The checker generates many inputs (including empty lists, zero and negative numbers) and, when the
property is false, reports the smallest counterexample it finds. `where it >= 1` restricts an input.
Laws are fixed by the task: do not change or copy them, change the code so they hold.
