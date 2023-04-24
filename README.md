# zero2prod

![Build pipeline](https://github.com/slowteetoe/zero2prod/actions/workflows/general.yml/badge.svg?event=push)

Working along the book examples from [Zero to Production in Rust by Luca Palmieri](https://www.zero2prod.com/)

## Testing

We are using the `bunyan` CLI to prettify output logs. The original Bunyan requires NPM, but you can install a Rust port with:

```sh
cargo install bunyan
```

To see the logs:

```sh
TEST_LOG=true cargo test | bunyan
```
