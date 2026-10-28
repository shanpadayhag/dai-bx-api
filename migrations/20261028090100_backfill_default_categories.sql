INSERT INTO categories (id, user_id, name, color)
SELECT gen_random_uuid(), users.id, default_category.name, default_category.color
FROM users
CROSS JOIN (
    VALUES
        ('Goals', '#7c95bd'),
        ('Personal', '#3a405a'),
        ('Studies', '#be7e86'),
        ('Work', '#c6afa3')
) AS default_category (name, color)
ON CONFLICT (user_id, name) DO NOTHING;
