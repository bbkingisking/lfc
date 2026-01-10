# lfc

Scrapes news about Liverpool FC from several websites, summarizes them using AI, and delivers the summaries via email and Telegram.

## Example output

```md
The news for Liverpool over the past 24 hours is mostly positive, showcasing the club's triumphant title win and the impactful performances of key players, despite some lingering concerns over player futures.

- Liverpool has officially secured the Premier League title after a thrilling 5-1 comeback victory against Tottenham Hotspur, marking their 20th league title and the first celebrated in front of fans at Anfield since 1990. 🎉🏆

- Mohamed Salah expressed that winning the title without Jurgen Klopp feels "way better," highlighting the special nature of this season's triumph and his impressive contributions to the team's success. 💪⚽️

- The midfield trio of Alexis Mac Allister, Dominik Szoboszlai, and Ryan Gravenberch have been pivotal, showing remarkable stability and performance throughout the season, contributing significantly to Liverpool's title charge. 🛠️

- Despite the title win, ongoing transfer speculation surrounds key players like Darwin Nunez, who is reportedly considering a move away from the club, raising questions about potential squad changes for next season. 🔄

- Liverpool's manager Arne Slot has received praise for his effective management style, enabling the team to thrive in the absence of previous stars, and is now focused on reinforcing the squad for the upcoming transfer window. 🌟
```
## Installation

Make sure you have Rust installed, then: 

1. `cargo install lfc`

## Config

The configuration lives in config.yaml file in XDG_CONFIG_HOME (so ~/.config/lfc/ on Unix). It looks like this:

```yaml
api_key: OPEN-AI-API-KEY-HERE
model: gpt-5-mini # or any gpt-5 series model
db_path: path/to/db.sqlite # stores the scraped articles in SQLite

emails:
  - email_1
  - email_2

telegram_chat_ids:
  - chat_id_1
  - chat_id_2
  
telegram_bot_token: token_here
email_username: email_credentials
email_app_password: email_credentials
```

## Tips

cron it on a daily schedule.
