CREATE TABLE IF NOT EXISTS address (
    id SERIAL PRIMARY KEY,
    uuid UUID NOT NULL UNIQUE,
    created TIMESTAMP NOT NULL,
    last_edited TIMESTAMP NOT NULL,
    building TEXT NOT NULL,
    street TEXT,
    town_or_city TEXT,
    postcode TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS person (
    id SERIAL PRIMARY KEY,
    uuid UUID NOT NULL UNIQUE,
    created TIMESTAMP NOT NULL,
    last_edited TIMESTAMP NOT NULL,
    first_name TEXT NOT NULL,
    family_name TEXT NOT NULL,
    date_of_birth DATE NOT NULl,
    address UUID,
    FOREIGN KEY (address) REFERENCES address (uuid)
);