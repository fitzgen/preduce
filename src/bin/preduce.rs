//! The `preduce` executable.

#![deny(missing_docs)]

extern crate clap;
extern crate preduce;

use std::process;

fn main() {
    if let Err(e) = try_main() {
        println!("Error: {}", e);
        process::exit(1);
    }
}

fn parse_args() -> clap::ArgMatches<'static> {
    clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            clap::Arg::with_name("test-case")
                .required(true)
                .help("The initial test case to reduce.")
        )
        .arg(
            clap::Arg::with_name("predicate")
                .required(true)
                .help("The is-interesting predicate script.")
        )
        .arg(
            clap::Arg::with_name("reducer")
                .required(true)
                .multiple(true)
                .min_values(1)
                .help("The reduction generator scripts. There must be at least one.")
        )
        .arg(
            clap::Arg::with_name("workers")
                .short("w")
                .long("workers")
                .takes_value(true)
                .value_name("NUM_WORKERS")
                .validator(
                    |a| {
                        let num = a.parse::<usize>().map_err(|e| format!("{}", e))?;
                        if num > 0 {
                            Ok(())
                        } else {
                            Err("NUM_WORKERS must be a number greater than 0".into())
                        }
                    }
                )
                .help(
                    "Set the number of parallel workers. Defaults to the number of logical \
                        CPUs."
                )
        )
        .arg(
            clap::Arg::with_name("no_merging")
                .short("n")
                .long("no-merging")
                .help("Do not attempt to do merges of interesting test cases.")
        )
        .get_matches()
}

fn try_main() -> preduce::error::Result<()> {
    let args = parse_args();

    let predicate = args.value_of("predicate").unwrap();
    let predicate = preduce::interesting::Script::new(predicate)?;

    let mut reducers = args.values_of("reducer").unwrap();
    let reducer = match (reducers.next(), reducers.next()) {
        (Some(r), None) => Box::new(preduce::reducers::Script::new(r)?) as Box<preduce::traits::Reducer>,
        (Some(r1), Some(r2)) => {
            let init = Box::new(preduce::reducers::Chain::new(preduce::reducers::Script::new(r1)?,
                                                              preduce::reducers::Script::new(r2)?));
            let init = init as Box<preduce::traits::Reducer>;
            let chained: preduce::error::Result<Box<preduce::traits::Reducer>> =
                reducers.fold(Ok(init), |acc, r| {
                    let acc = acc?;
                    let script = preduce::reducers::Script::new(r)?;
                    let chained = preduce::reducers::Chain::new(acc, script);
                    Ok(Box::new(chained) as Box<preduce::traits::Reducer>)
                });
            chained?
        }
        _ => unreachable!(),
    };

    let test_case = args.value_of("test-case").unwrap();

    let mut options = preduce::Options::new(predicate, reducer, test_case);

    if let Some(num_workers) = args.value_of("workers") {
        let num_workers = num_workers.parse::<usize>().unwrap();
        options = options.workers(num_workers);
    }

    if args.is_present("no_merging") {
        options = options.try_merging(false);
    }

    options.run()
}
