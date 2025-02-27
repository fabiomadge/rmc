use crate::builder::{Builder, RunConfig, ShouldRun, Step};
use crate::dist::distdir;
use crate::tool::Tool;
use crate::{compile, tool, Compiler};
use build_helper::output;
use std::process::Command;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BookRunner {
    pub compiler: Compiler,
}

impl Step for BookRunner {
    type Output = ();

    /// Runs the `bookrunner` tool.
    ///
    /// This tool in `src/tools` extracts examples from books, runs them through
    /// RMC, and displays their results.
    fn run(self, builder: &Builder<'_>) {
        // Before running `bookrunner`, we need to ensure that it is already
        // built.
        let book_runner = builder.ensure(tool::BookRunner { compiler: self.compiler });
        // We also need to ensure that stage n standard library is built for
        // rmc-rustc.
        builder.ensure(compile::Std { compiler: self.compiler, target: self.compiler.host });
        let target = builder.config.build.triple;
        builder.info("Launching book runner...");
        try_run(builder, Command::new(book_runner).env("TRIPLE", target));
    }

    fn should_run(run: ShouldRun<'_>) -> ShouldRun<'_> {
        run.path("src/tools/bookrunner")
    }

    fn make_run(run: RunConfig<'_>) {
        run.builder.ensure(BookRunner {
            compiler: run.builder.compiler(run.builder.top_stage, run.target),
        });
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ExpandYamlAnchors;

impl Step for ExpandYamlAnchors {
    type Output = ();

    /// Runs the `expand-yaml_anchors` tool.
    ///
    /// This tool in `src/tools` reads the CI configuration files written in YAML and expands the
    /// anchors in them, since GitHub Actions doesn't support them.
    fn run(self, builder: &Builder<'_>) {
        builder.info("Expanding YAML anchors in the GitHub Actions configuration");
        try_run(
            builder,
            &mut builder.tool_cmd(Tool::ExpandYamlAnchors).arg("generate").arg(&builder.src),
        );
    }

    fn should_run(run: ShouldRun<'_>) -> ShouldRun<'_> {
        run.path("src/tools/expand-yaml-anchors")
    }

    fn make_run(run: RunConfig<'_>) {
        run.builder.ensure(ExpandYamlAnchors);
    }
}

fn try_run(builder: &Builder<'_>, cmd: &mut Command) -> bool {
    if !builder.fail_fast {
        if !builder.try_run(cmd) {
            let mut failures = builder.delayed_failures.borrow_mut();
            failures.push(format!("{:?}", cmd));
            return false;
        }
    } else {
        builder.run(cmd);
    }
    true
}

#[derive(Debug, PartialOrd, Ord, Copy, Clone, Hash, PartialEq, Eq)]
pub struct BuildManifest;

impl Step for BuildManifest {
    type Output = ();
    const ONLY_HOSTS: bool = true;

    fn should_run(run: ShouldRun<'_>) -> ShouldRun<'_> {
        run.path("src/tools/build-manifest")
    }

    fn make_run(run: RunConfig<'_>) {
        run.builder.ensure(BuildManifest);
    }

    fn run(self, builder: &Builder<'_>) {
        // This gets called by `promote-release`
        // (https://github.com/rust-lang/promote-release).
        let mut cmd = builder.tool_cmd(Tool::BuildManifest);
        let sign = builder.config.dist_sign_folder.as_ref().unwrap_or_else(|| {
            panic!("\n\nfailed to specify `dist.sign-folder` in `config.toml`\n\n")
        });
        let addr = builder.config.dist_upload_addr.as_ref().unwrap_or_else(|| {
            panic!("\n\nfailed to specify `dist.upload-addr` in `config.toml`\n\n")
        });

        let today = output(Command::new("date").arg("+%Y-%m-%d"));

        cmd.arg(sign);
        cmd.arg(distdir(builder));
        cmd.arg(today.trim());
        cmd.arg(addr);
        cmd.arg(&builder.config.channel);

        builder.create_dir(&distdir(builder));
        builder.run(&mut cmd);
    }
}

#[derive(Debug, PartialOrd, Ord, Copy, Clone, Hash, PartialEq, Eq)]
pub struct BumpStage0;

impl Step for BumpStage0 {
    type Output = ();
    const ONLY_HOSTS: bool = true;

    fn should_run(run: ShouldRun<'_>) -> ShouldRun<'_> {
        run.path("src/tools/bump-stage0")
    }

    fn make_run(run: RunConfig<'_>) {
        run.builder.ensure(BumpStage0);
    }

    fn run(self, builder: &Builder<'_>) -> Self::Output {
        let mut cmd = builder.tool_cmd(Tool::BumpStage0);
        builder.run(&mut cmd);
    }
}
