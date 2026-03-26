from solution import is_pangram


def test_empty_sentence():
    assert is_pangram("") is False


def test_perfect_lower_case():
    assert is_pangram("abcdefghijklmnopqrstuvwxyz") is True


def test_only_lower_case():
    assert is_pangram("the quick brown fox jumps over the lazy dog") is True


def test_missing_letter_x():
    assert is_pangram("a quick movement of the enemy will jeopardize five gunboats") is False


def test_missing_letter_h():
    assert is_pangram("five boxing wizards jump quickly at my request") is False


def test_with_underscores():
    assert is_pangram("the_quick_brown_fox_jumps_over_the_lazy_dog") is True


def test_with_numbers():
    assert is_pangram("the 1 quick brown fox jumps over the 2 lazy dogs") is True


def test_missing_letters_replaced_by_numbers():
    assert is_pangram("7h3 qu1ck brown fox jumps ov3r 7h3 lazy dog") is False


def test_mixed_case_and_punctuation():
    assert is_pangram('"Five quacking Zephyrs jolt my wax bed."') is True


def test_upper_and_lower_case():
    assert is_pangram("the quick brown fox jumps over with lazy FX") is False
