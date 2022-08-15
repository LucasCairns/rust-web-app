CREATE TABLE IF NOT EXISTS address (
    id SERIAL PRIMARY KEY,
    uuid UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    created TIMESTAMP NOT NULL DEFAULT now(),
    last_edited TIMESTAMP NOT NULL DEFAULT now(),
    building TEXT NOT NULL,
    street TEXT,
    town_or_city TEXT,
    postcode TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS person (
    id SERIAL PRIMARY KEY,
    uuid UUID UNIQUE NOT NULL DEFAULT gen_random_uuid(),
    created TIMESTAMP NOT NULL DEFAULT now(),
    last_edited TIMESTAMP NOT NULL DEFAULT now(),
    first_name TEXT NOT NULL,
    family_name TEXT NOT NULL,
    date_of_birth DATE NOT NULl,
    address UUID,
    FOREIGN KEY (address) REFERENCES address (uuid)
);