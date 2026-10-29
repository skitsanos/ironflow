pub const MRAP: &str = "arn:aws:s3::123456789012:accesspoint/fixture.mrap";

pub fn command(directory: &std::path::Path) -> tokio::process::Command {
    for file in ["aws-config", "aws-credentials"] {
        std::fs::write(directory.join(file), "").unwrap();
    }
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .env("HOME", directory)
        .env("TMPDIR", directory)
        .env("AWS_ACCESS_KEY_ID", "fixture-access")
        .env("AWS_SECRET_ACCESS_KEY", "fixture-secret")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .env("AWS_CONFIG_FILE", directory.join("aws-config"))
        .env(
            "AWS_SHARED_CREDENTIALS_FILE",
            directory.join("aws-credentials"),
        )
        .current_dir(directory)
        .kill_on_drop(true);
    command
}
