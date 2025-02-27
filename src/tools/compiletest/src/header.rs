// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Modifications Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// See GitHub history for details.

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use tracing::*;

use crate::common::{Config, Mode, RMCFailStep};

#[derive(Clone, Debug)]
pub struct TestProps {
    // Extra flags to pass to the compiler
    pub compile_flags: Vec<String>,
    // Extra flags to pass to RMC
    pub rmc_flags: Vec<String>,
    // Extra flags to pass to CBMC
    pub cbmc_flags: Vec<String>,
    // The step where RMC is expected to fail
    pub rmc_panic_step: Option<RMCFailStep>,
}

impl TestProps {
    pub fn new() -> Self {
        TestProps {
            compile_flags: vec![],
            rmc_flags: vec![],
            cbmc_flags: vec![],
            rmc_panic_step: None,
        }
    }

    pub fn from_file(testfile: &Path, cfg: Option<&str>, config: &Config) -> Self {
        let mut props = TestProps::new();
        props.load_from(testfile, cfg, config);

        props
    }

    /// Loads properties from `testfile` into `props`. If a property is
    /// tied to a particular revision `foo` (indicated by writing
    /// `//[foo]`), then the property is ignored unless `cfg` is
    /// `Some("foo")`.
    fn load_from(&mut self, testfile: &Path, cfg: Option<&str>, config: &Config) {
        let mut has_edition = false;
        if !testfile.is_dir() {
            let file = File::open(testfile).unwrap();

            iter_header(testfile, file, &mut |revision, ln| {
                if revision.is_some() && revision != cfg {
                    return;
                }

                if let Some(flags) = config.parse_compile_flags(ln) {
                    self.compile_flags.extend(flags.split_whitespace().map(|s| s.to_owned()));
                }

                if let Some(flags) = config.parse_rmc_flags(ln) {
                    self.rmc_flags.extend(flags.split_whitespace().map(|s| s.to_owned()));
                }

                if let Some(flags) = config.parse_cbmc_flags(ln) {
                    self.cbmc_flags.extend(flags.split_whitespace().map(|s| s.to_owned()));
                }

                if let Some(edition) = config.parse_edition(ln) {
                    self.compile_flags.push(format!("--edition={}", edition));
                    has_edition = true;
                }

                self.update_rmc_fail_mode(ln, config);
            });
        }

        if let (Some(edition), false) = (&config.edition, has_edition) {
            self.compile_flags.push(format!("--edition={}", edition));
        }
    }

    /// Checks if `ln` specifies which stage the test should fail on and updates
    /// RMC fail mode accordingly.
    fn update_rmc_fail_mode(&mut self, ln: &str, config: &Config) {
        let rmc_fail_step = config.parse_rmc_step_fail_directive(ln);
        match (self.rmc_panic_step, rmc_fail_step) {
            (None, Some(_)) => self.rmc_panic_step = rmc_fail_step,
            (Some(_), Some(_)) => panic!("multiple `rmc-*-fail` headers in a single test"),
            (_, None) => {}
        }
    }
}

fn iter_header<R: Read>(testfile: &Path, rdr: R, it: &mut dyn FnMut(Option<&str>, &str)) {
    if testfile.is_dir() {
        return;
    }

    let comment = if testfile.extension().map(|e| e == "rs") == Some(true) { "//" } else { "#" };

    let mut rdr = BufReader::new(rdr);
    let mut ln = String::new();

    loop {
        ln.clear();
        if rdr.read_line(&mut ln).unwrap() == 0 {
            break;
        }

        // Assume that any directives will be found before the first
        // module or function. This doesn't seem to be an optimization
        // with a warm page cache. Maybe with a cold one.
        let ln = ln.trim();
        if ln.starts_with("fn") || ln.starts_with("mod") {
            return;
        } else if ln.starts_with(comment) {
            it(None, ln[comment.len()..].trim_start());
        }
    }
}

impl Config {
    fn parse_compile_flags(&self, line: &str) -> Option<String> {
        self.parse_name_value_directive(line, "compile-flags")
    }

    /// Parses strings of the form `rmc-*-fail` and returns the step at which
    /// RMC is expected to panic.
    fn parse_rmc_step_fail_directive(&self, line: &str) -> Option<RMCFailStep> {
        let check_rmc = |mode: &str| {
            if self.mode != Mode::RMC {
                panic!("`rmc-{}-fail` header is only supported in RMC tests", mode);
            }
        };
        if self.parse_name_directive(line, "rmc-check-fail") {
            check_rmc("check");
            Some(RMCFailStep::Check)
        } else if self.parse_name_directive(line, "rmc-codegen-fail") {
            check_rmc("codegen");
            Some(RMCFailStep::Codegen)
        } else if self.parse_name_directive(line, "rmc-verify-fail") {
            check_rmc("verify");
            Some(RMCFailStep::Verify)
        } else {
            None
        }
    }

    /// Parses strings of the form `// rmc-flags: ...` and returns the options listed after `rmc-flags:`
    fn parse_rmc_flags(&self, line: &str) -> Option<String> {
        self.parse_name_value_directive(line, "rmc-flags")
    }

    /// Parses strings of the form `// cbmc-flags: ...` and returns the options listed after `cbmc-flags:`
    fn parse_cbmc_flags(&self, line: &str) -> Option<String> {
        self.parse_name_value_directive(line, "cbmc-flags")
    }

    fn parse_name_directive(&self, line: &str, directive: &str) -> bool {
        // Ensure the directive is a whole word. Do not match "ignore-x86" when
        // the line says "ignore-x86_64".
        line.starts_with(directive)
            && matches!(line.as_bytes().get(directive.len()), None | Some(&b' ') | Some(&b':'))
    }

    pub fn parse_name_value_directive(&self, line: &str, directive: &str) -> Option<String> {
        let colon = directive.len();
        if line.starts_with(directive) && line.as_bytes().get(colon) == Some(&b':') {
            let value = line[(colon + 1)..].to_owned();
            debug!("{}: {}", directive, value);
            Some(value)
        } else {
            None
        }
    }

    pub fn find_rust_src_root(&self) -> Option<PathBuf> {
        let mut path = self.src_base.clone();
        let path_postfix = Path::new("src/etc/lldb_batchmode.py");

        while path.pop() {
            if path.join(&path_postfix).is_file() {
                return Some(path);
            }
        }

        None
    }

    fn parse_edition(&self, line: &str) -> Option<String> {
        self.parse_name_value_directive(line, "edition")
    }
}

pub fn make_test_description<R: Read>(
    config: &Config,
    name: test::TestName,
    path: &Path,
    src: R,
    cfg: Option<&str>,
) -> test::TestDesc {
    let mut ignore = false;
    let mut should_fail = false;

    if config.mode == Mode::RMC || config.mode == Mode::Stub {
        // If the path to the test contains "fixme" or "ignore", skip it.
        let file_path = path.to_str().unwrap();
        ignore |= file_path.contains("fixme") || file_path.contains("ignore");
    }

    // The `RmcFixme` mode runs tests that are ignored in the `rmc` suite
    if config.mode == Mode::RmcFixme {
        let file_path = path.to_str().unwrap();

        // `file_path` is going to be `src/test/rmc-fixme/...` so we
        // need to extract the base name if we want to ignore it
        let test_name: Vec<&str> = file_path.rsplit('/').collect();
        let base_name = test_name[0];

        // If the base name does NOT contain "fixme" or "ignore", we skip it.
        // All "fixme" tests are expected to fail
        ignore |= !(base_name.contains("fixme") || base_name.contains("ignore"));
        should_fail = true;
    }

    iter_header(path, src, &mut |revision, ln| {
        if revision.is_some() && revision != cfg {
            return;
        }
        should_fail |= config.parse_name_directive(ln, "should-fail");
    });

    // The `should-fail` annotation doesn't apply to pretty tests,
    // since we run the pretty printer across all tests by default.
    // If desired, we could add a `should-fail-pretty` annotation.
    let should_panic = match config.mode {
        _ if should_fail => test::ShouldPanic::Yes,
        _ => test::ShouldPanic::No,
    };

    test::TestDesc {
        name,
        ignore,
        should_panic,
        allow_fail: false,
        compile_fail: false,
        no_run: false,
        test_type: test::TestType::Unknown,
    }
}
