# rust-web-app

simple rust API built using axum and sqlx

---

## Pre-requisites

- [Rust ^1.62.0](https://www.rust-lang.org/tools/install)
- [Docker](https://www.docker.com/products/docker-desktop/)

## Running the application

You will be required to have Postgres database set up and running to allow for the compile time validation of the SQL statements

```sh
docker-compose up
```

**Note:** You may be required to create the database schema specified in the `.env` file if it does not exist already.

Install `sqlx-cli` if not already installed

```
cargo install sqlx-cli
```

Apply the migrations

```
sqlx migrate run --source=db/migrations
```

Start the application

```
cargo run
```

The application should now be running on [localhost:8080](http://localhost:8080) along with the [swagger docs](http://localhost:8080/swagger-ui/)
