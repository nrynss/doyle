// Wonder's Rust rewrite of BILLBATCH (story fixture, Trap D).
//
// This is the WRONG rewrite: Wonder ported the batch carrying L1, its
// remembered rule "1.5% on the whole balance, no exceptions", so it
// charges every account and never checks VISIT-CODE. Against the v1
// COBOL run it mismatches exactly the 535 vaccination-visit records.
//
// Reads the v1 fixed-width file (34-char records), writes the same
// detail lines and TOTALS line the COBOL report uses. Build with
// rustc (no crates):  rustc -O wonder-bill.rs -o /tmp/wonder-bill
// Run in v1/ (filenames are fixed):  /tmp/wonder-bill
use std::fs;
use std::io::Write;

fn money(cents: i64, width: usize) -> String {
    // COBOL edited-picture rule ($$$,$$9.99 style): one floating $ and
    // one comma, the comma before the last three integer digits.
    let int = format!("{}", cents / 100);
    let int = if int.len() > 3 {
        let (hi, lo) = int.split_at(int.len() - 3);
        format!("{},{}", hi, lo)
    } else {
        int
    };
    let out = format!("${}.{:02}", int, cents.rem_euclid(100));
    format!("{:>width$}", out, width = width)
}

fn fee_cents(balance_cents: i64) -> i64 {
    (balance_cents * 15 + 500) / 1000
}

fn main() {
    let data = fs::read_to_string("patients.dat").expect("patients.dat");
    let mut out = fs::File::create("rewrite-report.txt").expect("output");
    let mut total_balance: i64 = 0;
    let mut total_fee: i64 = 0;
    let mut count = 0;
    for line in data.lines() {
        if line.is_empty() {
            continue;
        }
        let acct = &line[0..6];
        let name = &line[6..26];
        let balance: i64 = line[26..33].parse().expect("balance");
        // BUG (L1): no VISIT-CODE check. Every account pays.
        let fee = fee_cents(balance);
        total_balance += balance;
        total_fee += fee;
        count += 1;
        writeln!(
            out,
            "ACCT {} NAME  {} BALANCE {} FEE {}",
            acct,
            name,
            money(balance, 10),
            money(fee, 10)
        )
        .expect("write");
    }
    writeln!(
        out,
        "TOTALS RECORDS  {:>4} BALANCE {} FEE {}",
        count,
        money(total_balance, 12),
        money(total_fee, 10)
    )
    .expect("write");
}
