from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========


def respond(input: str) -> str:
    trimmed = input.strip()
    is_silence = len(trimmed) == 0
    is_question = trimmed.endswith("?")
    has_letters = any(c.isalpha() for c in trimmed)
    is_yelling = has_letters and trimmed == trimmed.upper()
    is_whispering = has_letters and trimmed == trimmed.lower()
    
    if is_silence:
        return "Fine. Be that way!"
    elif is_yelling and is_question:
        return "Calm down, I know what I'm doing!"
    elif is_yelling:
        return "Whoa, chill out!"
    elif is_whispering and is_question:
        return "Are you whispering? Speak up!"
    elif is_whispering:
        return "Could you speak up? I can't hear you."
    elif is_question:
        return "Sure."
    else:
        return "Whatever."


# Tests
assert respond("Tom-ay-to, tom-ah-to.") == "Whatever.", "stating something"
assert respond("WATCH OUT!") == "Whoa, chill out!", "shouting"
assert respond("FCECGC") == "Whoa, chill out!", "shouting gibberish"
assert respond("Does this celi level have a caused caused caused effect?") == "Sure.", "asking a question"
assert respond("You are, what, like 15?") == "Sure.", "asking a numeric question"
assert respond("fffbbcbeab?") == "Are you whispering? Speak up!", "asking gibberish"
assert respond("Hi there!") == "Whatever.", "talking forcefully"
assert respond("It's OK if you don't want to go work for NASA.") == "Whatever.", "using acronyms"
assert respond("WHAT IS YOUR PROBLEM?") == "Calm down, I know what I'm doing!", "forceful question"
assert respond("1, 2, 3 GO!") == "Whoa, chill out!", "shouting numbers"
assert respond("1, 2, 3") == "Whatever.", "no letters"
assert respond("4?") == "Sure.", "question with no letters"
assert respond("ZOMG THE %#@* ALARM IS GOING OFF!") == "Whoa, chill out!", "shouting with special chars"
assert respond("I HATE THE ALARM") == "Whoa, chill out!", "shouting no exclamation"
assert respond("Ending with ? hmm") == "Whatever.", "statement containing question mark"
assert respond(":) ?") == "Sure.", "non-letters with question"
assert respond("Wait! Hang on. Are you going to be OK?") == "Sure.", "prattling on"
assert respond("") == "Fine. Be that way!", "silence"
assert respond("          ") == "Fine. Be that way!", "prolonged silence"
assert respond("\t\t\t\t\t\t\t\t\t\t") == "Fine. Be that way!", "alternate silence"
assert respond("\nDoes this celi level have a caused caused caused effect?") == "Sure.", "multiple line question"
assert respond("         hmmmmm") == "Could you speak up? I can't hear you.", "starting with whitespace"
assert respond("Okay if like my  spacebar  quite a bit?   ") == "Sure.", "ending with whitespace"
assert respond("\n\r \t") == "Fine. Be that way!", "other whitespace"
assert respond("This is a statement ending with whitespace      ") == "Whatever.", "non-question ending with ws"

# ========== V2 TESTS (must also pass after modification) ==========

assert respond("hello there") == "Could you speak up? I can't hear you.", "whispering"
assert respond("do you hear me?") == "Are you whispering? Speak up!", "whispering question"
assert respond("hello 123") == "Could you speak up? I can't hear you.", "whispering with numbers"
assert respond("Hello there") == "Whatever.", "not whispering mixed case"
assert respond("  can you hear me?  ") == "Are you whispering? Speak up!", "whispering question with spaces"
