use miette::Result;

fn main() -> Result<()> {
    // Diagnostics render at a fixed width, so long paths wrap mid-word and
    // shift their wrap point with every new tempdir. A wide fixed handler
    // keeps messages stable across terminals and CI.
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(miette::GraphicalReportHandler::new().with_width(200))
    }));
    vdl::cli::run()
}
