use console::style;
use git2::Repository;
use std::env;
use std::error::Error;
use std::rc::Rc;

use gh_stack::api::PullRequest;
use gh_stack::graph::FlatDep;
use gh_stack::util::loop_until_confirm;
use gh_stack::Credentials;
use gh_stack::{api, git, graph, markdown, persist};

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct CommonArgs {
    /// All pull requests containing this identifier in their title form a stack
    identifier: String,

    /// Exclude an issue from consideration (by number). Pass multiple times
    #[arg(long = "excl", short = 'e', value_name = "ISSUE")]
    exclude: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Annotate the descriptions of all PRs in a stack with metadata about all PRs in the stack
    Annotate {
        #[command(flatten)]
        common: CommonArgs,

        /// Prepend the annotation with the contents of this file
        #[arg(long, short = 'p', value_name = "FILE")]
        prelude: Option<String>,
    },

    /// Print a list of all pull requests in a stack to STDOUT
    Log {
        #[command(flatten)]
        common: CommonArgs,
    },

    /// Print a bash script to STDOUT that can rebase/update the stack (with a little help)
    Rebase {
        #[command(flatten)]
        common: CommonArgs,
    },

    /// Rebuild a stack based on changes to local branches and mirror these changes up to the remote
    Autorebase {
        #[command(flatten)]
        common: CommonArgs,

        /// Name of the remote to (force-)push the updated stack to (default: `origin`)
        #[arg(
            default_value = "origin",
            long,
            short = 'r',
            value_name = "REMOTE",
            required = false
        )]
        remote: String,

        /// Path to a local copy of the repository
        #[arg(long, short = 'C', value_name = "PATH_TO_REPO")]
        repo: String,

        /// Stop the initial cherry-pick at this SHA (exclusive)
        #[arg(long = "initial-cherry-pick-boundary", short = 'b', value_name = "SHA")]
        boundary: Option<String>,
    },
}

async fn build_pr_stack(
    pattern: &str,
    credentials: &Credentials,
    exclude: Vec<String>,
) -> Result<FlatDep, Box<dyn Error>> {
    let prs = api::search::fetch_pull_requests_matching(pattern, &credentials).await?;

    let prs = prs
        .into_iter()
        .filter(|pr| !exclude.contains(&pr.number().to_string()))
        .map(Rc::new)
        .collect::<Vec<Rc<PullRequest>>>();
    let graph = graph::build(&prs);
    let stack = graph::log(&graph);
    Ok(stack)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::from_filename(".gh-stack").ok();

    let token = env::var("GHSTACK_OAUTH_TOKEN")
        .or(env::var("GH_TOKEN"))
        .expect("You didn't pass `GHSTACK_OAUTH_TOKEN or GH_TOKEN`");
    let credentials = Credentials::new(&token);
    let cli = Cli::parse();

    match &cli.command {
        Commands::Annotate { common, prelude } => {
            let identifier = common.identifier.clone();
            let excluded = common.exclude.clone();
            let prelude_path: Option<&str> = match prelude {
                Some(path) => Some(path.as_str()),
                None => None,
            };

            let stack = build_pr_stack(&identifier, &credentials, excluded).await?;
            let table = markdown::build_table(&stack, &identifier, prelude_path);

            for (pr, _) in stack.iter() {
                println!("{}: {}", pr.number(), pr.title());
            }
            loop_until_confirm("Going to update these PRs ☝️ ");

            persist::persist(&stack, &table, &credentials).await?;

            println!("Done!");
        }
        Commands::Log { common } => {
            let identifier = common.identifier.clone();
            let excluded = common.exclude.clone();
            let stack = build_pr_stack(&identifier, &credentials, excluded).await?;

            for (pr, maybe_parent) in stack {
                match maybe_parent {
                    Some(parent) => {
                        let into = style(format!("(Merges into #{})", parent.number())).green();
                        println!("#{}: {} {}", pr.number(), pr.title(), into);
                    }

                    None => {
                        let into = style("(Base)").red();
                        println!("#{}: {} {}", pr.number(), pr.title(), into);
                    }
                }
            }
        }
        Commands::Rebase { common } => {
            let identifier = common.identifier.to_string();
            let excluded = common.exclude.clone();

            let stack = build_pr_stack(&identifier, &credentials, excluded).await?;

            let script = git::generate_rebase_script(stack);
            println!("{}", script);
        }
        Commands::Autorebase {
            common,
            remote,
            repo,
            boundary,
        } => {
            let identifier = common.identifier.clone();
            let excluded = common.exclude.clone();
            let boundary_str: Option<&str> = match boundary {
                Some(b) => Some(b.as_str()),
                None => None,
            };

            let stack = build_pr_stack(&identifier, &credentials, excluded).await?;
            let repo = Repository::open(repo)?;
            let remote = repo.find_remote(remote).unwrap();
            git::perform_rebase(stack, &repo, remote.name().unwrap(), boundary_str).await?;
            println!("All done");
        }
    }

    Ok(())
    /*
    # TODO
    - [x] Authentication (personal access token)
    - [x] Fetch all PRs matching Jira
    - [x] Construct graph
    - [x] Create markdown table
    - [x] Persist table back to Github
    - [x] Accept a prelude via STDIN
    - [x] Log a textual representation of the graph
    - [x] Automate rebase
    - [x] Better CLI args
    - [ ] Build status icons
    - [ ] Panic on non-200s
    */
}
