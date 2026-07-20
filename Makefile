SHELL := /bin/bash
.DEFAULT_GOAL := help

include .env
export DATABASE_URL AUTH_URL ISSUER_URL AUDIENCE

SQLX            := sqlx migrate run --source=db/migrations
DOCKER_COMPOSE  := docker compose
KEYCLOAK_ADMIN  ?= admin
KC_URL          := http://localhost:8081

.PHONY: all up down build lint test clean help

## Default target - ensure infra is ready and compile
all: up build

help:
	@echo "Available targets:"
	@echo "  make all    Start services, apply migrations, and compile (default)"
	@echo "  make up     Start PostgreSQL + Keycloak, wait for readiness, run migrations"
	@echo "  make down   Stop and remove containers"
	@echo "  make build  Compile the project (services + migrations first)"
	@echo "  make lint   Run clippy lints (services + migrations first)"
	@echo "  make test   Run all tests (services + migrations first)"
	@echo "  make clean  Stop services and remove build artefacts"

up:
	@echo "Starting PostgreSQL and Keycloak ..."
	$(DOCKER_COMPOSE) up -d postgres keycloak
	@echo "Waiting for PostgreSQL readiness ..."
	@until pg_isready -h localhost -p 5432 >/dev/null 2>&1; do sleep 1; done
	@echo "Waiting for Keycloak readiness ..."
	@until curl -sf $(KC_URL)/realms/master >/dev/null 2>&1; do sleep 2; done
	@echo "Setting up Keycloak realm and client ..."
	@bash scripts/setup-keycloak.sh
	@echo "Applying pending migrations ..."
	$(SQLX)

down:
	$(DOCKER_COMPOSE) down

build: up
	cargo build

lint: up
	cargo clippy --all-targets

test: up
	cargo test

clean: down
	cargo clean
