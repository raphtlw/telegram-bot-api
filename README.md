> proxy for telegram-bot-api with file cleanup capabilities

this repository contains a proxy written in rust and actix web which sits in front of the aiogram/telegram-bot-api docker container

motivation: i hosted the telegram-bot-api server but storage fills up quickly. to avoid needing to manually clean it every week i decided to build a reverse proxy which serves the files off the downloaded path and deletes them afterward.

previously, cloudflare tunnel served this purpose. it served files from the /var/lib/telegram-bot-api/files folder so the client could download it by hitting that url. however, i couldn't implement custom logic like to delete the file after the client fetches it, so i made this.
