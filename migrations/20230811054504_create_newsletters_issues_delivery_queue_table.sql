CREATE TABLE newsletters_issues_delivery_queue (
    id uuid NOT NULL REFERENCES newsletters_issues(id),
    subscriber_email text NOT NULL,
    PRIMARY KEY (id, subscriber_email)
)