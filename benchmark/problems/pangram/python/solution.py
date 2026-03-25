def is_pangram(sentence: str) -> bool:
    lower = sentence.lower()
    return all(c in lower for c in "abcdefghijklmnopqrstuvwxyz")
