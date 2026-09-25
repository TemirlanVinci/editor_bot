-- Create hashtags table and seed base Reddit hashtags (RU & EN)
CREATE TABLE IF NOT EXISTS hashtags (
    id SERIAL PRIMARY KEY,
    tag VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO hashtags (tag)
VALUES -- Russian Reddit hashtags
    ('#реддит'),
    ('#реддитистории'),
    ('#историисреддита'),
    ('#реддитпосты'),
    ('#аскреддит'),
    ('#тредыреддит'),
    ('#историиизжизни'),
    ('#реддитканал'),
    ('#историиреддит'),
    ('#озвучкареддит'),
    ('#переписка'),
    ('#истории'),
    ('#тренды'),
    -- English Reddit hashtags
    ('#reddit'),
    ('#redditstories'),
    ('#askreddit'),
    ('#redditreadings'),
    ('#reddittok'),
    ('#redditstory'),
    ('#redditposts'),
    ('#redditstorytime'),
    ('#redditmemes'),
    ('#redditchats'),
    ('#redditinsider'),
    ('#redditcut') ON CONFLICT (tag) DO NOTHING;