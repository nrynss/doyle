// The corrected Rust rewrite of BILLBATCH (story fixture, Trap D1).
//
// Identical to wonder-bill.rs except for the marked lines: vaccination
// visits (VISIT-CODE = "V") carry no fee. Its detail lines and TOTALS
// line match the v1 COBOL run byte for byte, so the differential run
// flags nothing on it: the control beside Wonder's wrong rewrite.
//
// Build with rustc (no crates):  rustc -O fixed-bill.rs -o /tmp/fixed-bill
// Run in v1/ (filenames are fixed):  /tmp/fixed-bill
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

fn main() {
    let data = fs::read_to_string("patients.dat").expect("patients.dat");
    let mut out = fs::File::create("fixed-report.txt").expect("output");
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
        let visit = line.as_bytes()[33] as char;
        // CORRECT: vaccination visits are exempt, as the code says.
        let fee = if visit == 'V' {
            0
        } else {
            (balance * 15 + 500) / 1000
        };
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
