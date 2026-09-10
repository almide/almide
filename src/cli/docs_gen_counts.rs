//! Live API claims follow module interfaces; dated totals follow the release ledger.

pub(super) struct Stamp {
    date: String,
    stdlib_functions: usize,
    stdlib_modules: usize,
}

pub(super) fn load_stamp() -> Result<Stamp, String> {
    let path = "proofs/ledger-counts.toml";
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let record: toml::Value =
        toml::from_str(&text).map_err(|e| format!("cannot parse {path}: {e}"))?;
    let date = record
        .get("date")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{path}: missing string date"))?
        .to_owned();
    let count = |key: &str| -> Result<usize, String> {
        record
            .get(key)
            .and_then(toml::Value::as_integer)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("{path}: missing nonnegative integer {key}"))
    };
    Ok(Stamp {
        date,
        stdlib_functions: count("stdlib_functions")?,
        stdlib_modules: count("stdlib_modules")?,
    })
}

pub(super) fn check_claims(text: &str, live: &str, stamp: &Stamp) -> Vec<String> {
    let mut errors = Vec::new();
    let stamped = format!(
        "{} functions across {} modules",
        stamp.stdlib_functions, stamp.stdlib_modules
    );
    let marker = format!("<!-- counts:generated:start (as of {})", stamp.date);
    let mut in_stamp = false;
    for line in text.lines() {
        if line.starts_with("<!-- counts:generated:start") {
            if in_stamp {
                errors.push("nested stamped count block".into());
            }
            if !line.starts_with(&marker) {
                errors.push(format!(
                    "count block date must match ledger date {}",
                    stamp.date
                ));
            }
            in_stamp = true;
        } else if line == "<!-- counts:generated:end -->" {
            if !in_stamp {
                errors.push("count block end without a start".into());
            }
            in_stamp = false;
        } else if let Some(claim) = extract_claim(line) {
            let expected = if in_stamp { stamped.as_str() } else { live };
            if claim != expected {
                let source = if in_stamp {
                    "dated ledger"
                } else {
                    "module interfaces"
                };
                errors.push(format!(
                    "claims `{claim}` but the {source} total `{expected}`"
                ));
            }
        }
    }
    if in_stamp {
        errors.push("unterminated stamped count block".into());
    }
    errors
}

fn extract_claim(line: &str) -> Option<String> {
    let index = line.find(" functions across ")?;
    let head = &line[..index];
    let n_start = head
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let tail = &line[index + " functions across ".len()..];
    let m_end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    if !tail[..m_end].is_empty() && !tail[m_end..].trim_start().starts_with("modules") {
        return None;
    }
    Some(format!(
        "{} functions across {} modules",
        &head[n_start..],
        &tail[..m_end]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp() -> Stamp {
        Stamp {
            date: "2026-09-08".into(),
            stdlib_functions: 985,
            stdlib_modules: 43,
        }
    }

    const LIVE: &str = "986 functions across 43 modules";
    const BLOCK: &str = "<!-- counts:generated:start (as of 2026-09-08) -->\n985 functions across 43 modules\n<!-- counts:generated:end -->";

    #[test]
    fn dated_totals_can_precede_live_api_and_do_not_hide_later_claims() {
        assert!(check_claims(&format!("{LIVE}\n{BLOCK}\n{LIVE}"), LIVE, &stamp()).is_empty());
        assert!(!check_claims(
            &format!("{BLOCK}\n985 functions across 43 modules"),
            LIVE,
            &stamp()
        )
        .is_empty());
    }

    #[test]
    fn forged_dated_total_and_date_are_rejected() {
        assert!(!check_claims(&BLOCK.replace("985", "986"), LIVE, &stamp()).is_empty());
        assert!(
            !check_claims(&BLOCK.replace("2026-09-08", "2026-09-09"), LIVE, &stamp()).is_empty()
        );
    }

    #[test]
    fn malformed_count_blocks_are_rejected() {
        for text in [
            BLOCK.replace("<!-- counts:generated:end -->", ""),
            format!("<!-- counts:generated:start (as of 2026-09-08) -->\n{BLOCK}"),
            "<!-- counts:generated:end -->".into(),
        ] {
            assert!(!check_claims(&text, LIVE, &stamp()).is_empty());
        }
    }
}
