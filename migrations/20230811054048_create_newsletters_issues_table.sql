CREATE TABLE newsletters_issues (
    id uuid NOT NULL,
    title text NOT NULL,
    text_content text NOT NULL,
    html_content text NOT NULL,
    published_at timestamptz NOT NULL,
    PRIMARY KEY (id)
)