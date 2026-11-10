//! Reachability at declared runtime boundaries of the native self-host link.
//! A route is only a proposal: native_hof must check each actual call and
//! record its native-calls witness. Function references always keep bodies.

use crate::lower::LowerError;
use crate::{MirFunction, Op};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Edge {
    target: String,
    direct: bool,
}
#[derive(Debug, PartialEq)]
struct Plan {
    retained: BTreeSet<String>,
    runtime: BTreeSet<String>,
}

// A routing proposal only: every selected call must still pass the native
// runtime planner and contribute its native-calls witness before acceptance.
fn plan_links(
    graph: &BTreeMap<String, Vec<Edge>>,
    roots: &BTreeSet<String>,
    failed: &BTreeSet<String>,
    routable: &BTreeSet<String>,
) -> Result<Plan, String> {
    let mut affected = failed.clone();
    loop {
        let before = affected.len();
        for (name, edges) in graph {
            if edges.iter().any(|e| affected.contains(&e.target)) {
                affected.insert(name.clone());
            }
        }
        if before == affected.len() {
            break;
        }
    }
    let cuts: BTreeSet<_> = routable
        .intersection(&affected)
        .filter(|n| !roots.contains(*n))
        .cloned()
        .collect();
    let mut out = Plan {
        retained: BTreeSet::new(),
        runtime: BTreeSet::new(),
    };
    let mut todo: Vec<_> = roots.iter().cloned().collect();
    while let Some(name) = todo.pop() {
        if !out.retained.insert(name.clone()) {
            continue;
        }
        if failed.contains(&name) {
            return Err(name);
        }
        for edge in graph.get(&name).into_iter().flatten() {
            if edge.direct && cuts.contains(&edge.target) {
                out.runtime.insert(edge.target.clone());
            } else {
                todo.push(edge.target.clone());
            }
        }
    }
    Ok(out)
}

/// Keep the existing successful path byte-for-byte. Only a native lowering
/// or representation refusal permits proposing a declared runtime boundary;
/// frontend errors and name clashes never enter this map.
pub(crate) fn route(
    functions: &mut Vec<MirFunction>,
    roots: &BTreeSet<String>,
    refused: &BTreeMap<String, LowerError>,
) -> Result<BTreeSet<String>, LowerError> {
    if refused.is_empty() {
        return Ok(BTreeSet::new());
    }
    let graph: BTreeMap<String, Vec<Edge>> = functions
        .iter()
        .map(|f| {
            let edges = f
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::CallFn { name, .. } => Some(Edge {
                        target: name.clone(),
                        direct: true,
                    }),
                    Op::FuncRef { name, .. } => Some(Edge {
                        target: name.clone(),
                        direct: false,
                    }),
                    _ => None,
                })
                .collect();
            (f.name.clone(), edges)
        })
        .collect();
    let failed = refused.keys().cloned().collect();
    let routable = graph
        .keys()
        .chain(refused.keys())
        .filter(|name| !roots.contains(*name) && crate::native_hof::of(name).is_some())
        .cloned()
        .collect();
    let plan =
        plan_links(&graph, roots, &failed, &routable).map_err(|name| refused[&name].clone())?;
    functions.retain(|f| plan.retained.contains(&f.name));
    Ok(plan.runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }
    fn graph(rows: &[(&str, &[(&str, bool)])]) -> BTreeMap<String, Vec<Edge>> {
        rows.iter()
            .map(|(n, es)| {
                (
                    n.to_string(),
                    es.iter()
                        .map(|(n, direct)| Edge {
                            target: n.to_string(),
                            direct: *direct,
                        })
                        .collect(),
                )
            })
            .collect()
    }
    #[test]
    fn a_declared_boundary_can_remove_an_unreachable_failed_helper() {
        let g = graph(&[("main", &[("filter", true)]), ("filter", &[("copy", true)])]);
        assert_eq!(
            plan_links(&g, &set(&["main"]), &set(&["copy"]), &set(&["filter"])),
            Ok(Plan {
                retained: set(&["main"]),
                runtime: set(&["filter"])
            })
        );
    }
    #[test]
    fn a_shared_helper_reached_by_an_unrouted_caller_still_refuses() {
        let g = graph(&[
            ("main", &[("filter", true), ("plain", true)]),
            ("filter", &[("copy", true)]),
            ("plain", &[("copy", true)]),
        ]);
        assert_eq!(
            plan_links(&g, &set(&["main"]), &set(&["copy"]), &set(&["filter"])),
            Err("copy".into())
        );
    }
    #[test]
    fn a_function_reference_cannot_be_replaced_by_a_direct_runtime_call() {
        let g = graph(&[
            ("main", &[("filter", false)]),
            ("filter", &[("copy", true)]),
        ]);
        assert_eq!(
            plan_links(&g, &set(&["main"]), &set(&["copy"]), &set(&["filter"])),
            Err("copy".into())
        );
    }
    #[test]
    fn a_reference_keeps_a_body_even_when_another_site_routes_it() {
        let g = graph(&[
            ("main", &[("filter", true), ("filter", false)]),
            ("filter", &[("copy", true)]),
        ]);
        assert!(plan_links(&g, &set(&["main"]), &set(&["copy"]), &set(&["filter"])).is_err());
    }
    #[test]
    fn a_successful_existing_link_does_not_change_routes() {
        let g = graph(&[("main", &[("filter", true)]), ("filter", &[("copy", true)])]);
        assert_eq!(
            plan_links(&g, &set(&["main"]), &set(&[]), &set(&["filter"])),
            Ok(Plan {
                retained: set(&["main", "filter", "copy"]),
                runtime: set(&[])
            })
        );
    }
    #[test]
    fn cycles_propagate_failures_and_terminate() {
        let g = graph(&[
            ("main", &[("filter", true)]),
            ("filter", &[("a", true)]),
            ("a", &[("b", true)]),
            ("b", &[("a", true), ("copy", true)]),
        ]);
        assert_eq!(
            plan_links(&g, &set(&["main"]), &set(&["copy"]), &set(&["filter"])),
            Ok(Plan {
                retained: set(&["main"]),
                runtime: set(&["filter"])
            })
        );
    }
    #[test]
    fn a_user_root_cannot_be_routed_away() {
        assert_eq!(
            plan_links(
                &BTreeMap::new(),
                &set(&["main"]),
                &set(&["main"]),
                &set(&["main"])
            ),
            Err("main".into())
        );
    }
}
