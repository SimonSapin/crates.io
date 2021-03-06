extern crate "cargo-registry" as cargo_registry;
extern crate "conduit-middleware" as conduit_middleware;
extern crate civet;
extern crate green;
extern crate git2;

use std::os;
use std::sync::Arc;
use std::io::{mod, fs, File};
use civet::Server;

fn main() {
    let url = env("GIT_REPO_URL");
    let checkout = Path::new(env("GIT_REPO_CHECKOUT"));

    let repo = match git2::Repository::open(&checkout) {
        Ok(r) => r,
        Err(..) => {
            let _ = fs::rmdir_recursive(&checkout);
            fs::mkdir_recursive(&checkout, io::USER_DIR).unwrap();
            let config = git2::Config::open_default().unwrap();
            let url = url.as_slice();
            cargo_registry::git::with_authentication(url, &config, |f| {
                let cb = git2::RemoteCallbacks::new().credentials(f);
                git2::build::RepoBuilder::new()
                                         .remote_callbacks(cb)
                                         .clone(url, &checkout)
            }).unwrap()
        }
    };
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", "bors").unwrap();
    cfg.set_str("user.email", "bors@rust-lang.org").unwrap();

    let heroku = os::getenv("HEROKU").is_some();
    let cargo_env = if heroku {
        cargo_registry::Production
    } else {
        cargo_registry::Development
    };
    let config = cargo_registry::Config {
        s3_bucket: env("S3_BUCKET"),
        s3_access_key: env("S3_ACCESS_KEY"),
        s3_secret_key: env("S3_SECRET_KEY"),
        s3_region: os::getenv("S3_REGION"),
        s3_proxy: None,
        session_key: env("SESSION_KEY"),
        git_repo_checkout: checkout,
        gh_client_id: env("GH_CLIENT_ID"),
        gh_client_secret: env("GH_CLIENT_SECRET"),
        db_url: env("DATABASE_URL"),
        env: cargo_env,
        max_upload_size: 10 * 1024 * 1024,
    };
    let app = cargo_registry::App::new(&config);
    let app = cargo_registry::middleware(Arc::new(app));

    // remove this after we've launched
    let mut app = conduit_middleware::MiddlewareBuilder::new(app);
    app.around(cargo_registry::util::LaunchGuard::new());

    let port = if heroku {
        8888
    } else {
        os::getenv("PORT").and_then(|s| from_str(s.as_slice()))
                          .unwrap_or(8888)
    };
    let _a = Server::start(civet::Config { port: port, threads: 8 }, app);
    println!("listening on port {}", port);
    if heroku {
        File::create(&Path::new("/tmp/app-initialized")).unwrap();
    }

    // TODO: handle a graceful shutdown by just waiting for a SIG{INT,TERM}
    let (_tx, rx) = channel::<()>();
    rx.recv();
}

fn env(s: &str) -> String {
    match os::getenv(s) {
        Some(s) => s,
        None => panic!("must have `{}` defined", s),
    }
}
