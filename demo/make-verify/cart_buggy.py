# The same mistake in Python: a 3rd parameter added, one call site missed.
def line(price, qty, discount):
    return price * qty - discount
def subtotal():
    return line(100, 2, 10) + line(50, 3)   # missing discount arg
print(subtotal())
