use flate2::read::GzDecoder;
use libcnb::{
    build::{cnb_runtime_build, GenericBuildContext},
    data,
};
use sha2::Digest;
use std::{
    collections::HashMap,
    env, fs, io,
    path::Path,
    process::{Command, Stdio},
};
use tar::Archive;
use tempfile::NamedTempFile;

const RUBY_URL: &str =
    "https://s3-external-1.amazonaws.com/heroku-buildpack-ruby/heroku-18/ruby-2.5.1.tgz";

fn main() -> anyhow::Result<()> {
    cnb_runtime_build(build);

    Ok(())
}

// need to add a logger / printing to stdout?
fn build(ctx: GenericBuildContext) -> anyhow::Result<()> {
    println!("---> Ruby Buildpack");

    println!("---> Download and extracting Ruby");
    let mut ruby_layer = ctx.layer("ruby")?;
    ruby_layer.mut_content_metadata().launch = true;
    ruby_layer.write_content_metadata()?;
    {
        let ruby_tgz = NamedTempFile::new()?;
        download(RUBY_URL, ruby_tgz.path())?;
        untar(ruby_tgz.path(), ruby_layer.as_path())?;
    }

    let mut ruby_env: HashMap<String, String> = HashMap::new();
    let ruby_bin_path = format!(
        "{}/.gem/ruby/2.6.6/bin",
        env::var("HOME").unwrap_or(String::new())
    );
    ruby_env.insert(
        String::from("PATH"),
        format!(
            "{}:{}:{}",
            ruby_layer.as_path().join("bin").as_path().to_str().unwrap(),
            ruby_bin_path,
            env::var("PATH").unwrap_or(String::new()),
        ),
    );
    ruby_env.insert(
        String::from("LD_LIBRARY_PATH"),
        format!(
            "{}:{}",
            env::var("LD_LIBRARY_PATH").unwrap_or(String::new()),
            ruby_layer
                .as_path()
                .join("layer")
                .as_path()
                .to_str()
                .unwrap()
        ),
    );
    println!("---> Installing bundler");
    {
        let cmd = Command::new("gem")
            .args(&["install", "bundler", "--no-ri", "--no-rdoc"])
            .envs(&ruby_env)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?
            .wait()?;
        if !cmd.success() {
            anyhow::anyhow!("Could not install bundler");
        }
    }

    let mut bundler_layer = ctx.layer("bundler")?;
    let bundler_layer_path = bundler_layer.as_path();
    let bundler_layer_binstubs_path = bundler_layer_path.join("bin");
    let local_checksum = toml::Value::String(format!(
        "{:x}",
        sha2::Sha256::digest(&fs::read("Gemfile.lock")?)
    ));
    let last_checksum = bundler_layer.content_metadata().metadata.get("checksum");
    if last_checksum == Some(&local_checksum) {
        println!("---> Reusing gems");
        Command::new("bundle")
            .args(&[
                "config",
                "--local",
                "path",
                bundler_layer_path.to_str().unwrap(),
            ])
            .envs(&ruby_env)
            .spawn()?
            .wait()?;
        Command::new("bundle")
            .args(&[
                "config",
                "--local",
                "bin",
                bundler_layer_binstubs_path.as_path().to_str().unwrap(),
            ])
            .envs(&ruby_env)
            .spawn()?
            .wait()?;
    } else {
        println!("---> Installing gems");
        let cmd = Command::new("bundle")
            .args(&[
                "install",
                "--path",
                bundler_layer_path.to_str().unwrap(),
                "--binstubs",
                bundler_layer_binstubs_path.as_path().to_str().unwrap(),
            ])
            .envs(&ruby_env)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?
            .wait()?;
        if !cmd.success() {
            anyhow::anyhow!("Could not bundle install");
        }

        let mut content_metadata = bundler_layer.mut_content_metadata();

        content_metadata.launch = true;
        content_metadata.cache = true;
        content_metadata
            .metadata
            .insert(String::from("checksum"), local_checksum);
        bundler_layer.write_content_metadata()?;
    }

    let mut launch_toml = data::launch::Launch::new();
    let web = data::launch::Process::new("web", "bundle", vec!["exec", "ruby", "app.rb"], false)?;
    let worker =
        data::launch::Process::new("worker", "bundle", vec!["exec", "ruby", "worker.rb"], false)?;
    launch_toml.processes.push(web);
    launch_toml.processes.push(worker);

    ctx.write_launch(launch_toml)?;

    Ok(())
}

fn download(uri: impl AsRef<str>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let response = reqwest::blocking::get(uri.as_ref())?;
    let mut content = io::Cursor::new(response.bytes()?);
    let mut file = fs::File::create(dst.as_ref())?;
    io::copy(&mut content, &mut file)?;

    Ok(())
}

fn untar(file: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let tar_gz = fs::File::open(file.as_ref())?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    archive.unpack(dst.as_ref())?;

    Ok(())
}
