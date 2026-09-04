//! One-shot Q7=B local stuck-preparation cleanup.
//!
//! Lists or resets failed / `action_required` preparation that lacks
//! `evaluation_sync` in (`ready`, `needs_decision`), plus optional
//! `--force-role-id` overrides. Never touches Applied / Discarded roles,
//! evaluation receipts, or career-ops.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn print_usage() {
    eprintln!(
        "Usage: reset-stuck-preparations --db <path> [--dry-run|--apply] [--force-role-id <uuid>]..."
    );
}

fn main() -> ExitCode {
    let mut db: Option<PathBuf> = None;
    let mut dry_run = true;
    let mut apply = false;
    let mut force_role_ids: Vec<String> = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => {
                let Some(path) = args.next() else {
                    print_usage();
                    return ExitCode::from(2);
                };
                db = Some(PathBuf::from(path));
            }
            "--dry-run" => {
                dry_run = true;
                apply = false;
            }
            "--apply" => {
                apply = true;
                dry_run = false;
            }
            "--force-role-id" => {
                let Some(role_id) = args.next() else {
                    print_usage();
                    return ExitCode::from(2);
                };
                force_role_ids.push(role_id);
            }
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                return ExitCode::from(2);
            }
        }
    }

    let Some(db_path) = db else {
        print_usage();
        return ExitCode::from(2);
    };
    if dry_run == apply {
        eprintln!("choose exactly one of --dry-run or --apply");
        return ExitCode::from(2);
    }

    let force_refs: Vec<&str> = force_role_ids.iter().map(String::as_str).collect();
    let candidates =
        match here_for_work_lib::list_stuck_preparation_cleanup_candidates(&db_path, &force_refs) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("list failed: {error}");
                return ExitCode::FAILURE;
            }
        };

    println!(
        "{}",
        serde_json::json!({
            "mode": if apply { "apply" } else { "dry-run" },
            "db": db_path,
            "forceRoleIds": force_role_ids,
            "candidates": candidates,
        })
    );

    if !apply {
        return ExitCode::SUCCESS;
    }

    match here_for_work_lib::reset_stuck_preparations(&db_path, &force_refs) {
        Ok(results) => {
            println!(
                "{}",
                serde_json::json!({
                    "mode": "apply-result",
                    "results": results,
                })
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("apply failed: {error}");
            ExitCode::FAILURE
        }
    }
}
