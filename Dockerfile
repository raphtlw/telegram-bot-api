FROM rust:latest

WORKDIR /telegram-bot-api-proxy

COPY . .

RUN cargo install --path .

CMD [ "telegram-bot-api-proxy" ]