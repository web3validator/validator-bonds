CREATE TABLE bonds (
  id BIGSERIAL NOT NULL,
  pubkey TEXT NOT NULL,
  vote_account TEXT NOT NULL,
  authority TEXT NOT NULL,
  revenue_share INTEGER NOT NULL,
  epoch INTEGER NOT NULL,
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
  PRIMARY KEY(id)
)