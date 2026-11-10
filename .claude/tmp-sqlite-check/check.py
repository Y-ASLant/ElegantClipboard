import sqlite3

print('sqlite version:', sqlite3.sqlite_version)
c = sqlite3.connect(':memory:')


def t(label, sql, params=()):
    try:
        rows = c.execute(sql, params).fetchall()
        print(label, '->', rows)
    except Exception as e:
        print(label, '-> ERROR:', type(e).__name__, e)


t('json_each(NULL) count', 'SELECT count(*) FROM json_each(NULL)')
t('json_each(bad) count', "SELECT count(*) FROM json_each('{bad')")
t('json_valid(bad)', "SELECT json_valid('{bad')")
t('json_array_length(bad)', "SELECT json_array_length('{bad')")
t('json_array_length(scalar)', 'SELECT json_array_length(\'"x.png"\')')
t('json_array_length(obj)', "SELECT json_array_length('{}')")
t('json_array_length(NULL)', 'SELECT json_array_length(NULL)')
t('json_each(scalar) value', 'SELECT value FROM json_each(\'"x.png"\')')
t('lower unicode', "SELECT lower('PNG ＰＮＧ Фото')")
t('dotfile path LIKE', "SELECT lower('C:\\\\.png') LIKE '%.png'")
t('dir.png trailing sep LIKE', "SELECT lower('C:\\\\dir.png\\\\') LIKE '%.png'")

EXTS = ['avif', 'bmp', 'dib', 'gif', 'heic', 'heif', 'ico', 'jfif', 'jpe', 'jpeg', 'jpg', 'png', 'tif', 'tiff', 'webp']
ext_sql = ' OR '.join(f"lower(json_each.value) LIKE '%.{e}'" for e in EXTS)
record_sql = (
    "json_valid(fp) AND json_array_length(fp) >= 1 AND NOT EXISTS ("
    f"SELECT 1 FROM json_each(fp) WHERE NOT ({ext_sql}))"
)


def trec(label, fp_json):
    sql = f"SELECT count(*) FROM (SELECT ? AS fp) WHERE {record_sql}"
    t(label, sql, (fp_json,))


trec('rec [".png"]', '[".png"]')
trec('rec ["a.png"]', '["a.png"]')
trec('rec ["a.png/", null]', '["a.png", null]')
trec('rec [null]', '[null]')
trec('rec [""]', '[""]')
trec('rec bad json', '{bad')
trec('rec scalar json', '"a.png"')
trec('rec empty array', '[]')
trec('rec ["dir.png\\\\"]', '["dir.png\\\\"]')
trec('rec ["x.PNG"]', '["x.PNG"]')
trec('rec ["x.png "]', '["x.png "]')
trec('rec ["x.png."]', '["x.png."]')
trec('rec unicode ext', '["x.\u30d5\u30eb\u30fc\u30c8"]')

# short-circuit check: does an invalid row kill the whole query when other rows are fine?
inner = "(SELECT '[\"a.png\"]' AS fp UNION ALL SELECT '{bad')"
t('mixed rows query',
  "SELECT count(*) FROM " + inner + " WHERE " + record_sql)
t('mixed rows query reversed guard',
  "SELECT count(*) FROM " + inner
  + " WHERE NOT EXISTS (SELECT 1 FROM json_each(fp) WHERE NOT (lower(value) LIKE '%.png'))"
    " AND json_valid(fp) AND json_array_length(fp) >= 1")

# LIKE escape behavior with backslash and percent in search
t('escape percent', r"SELECT 'a%b' LIKE '%a\%b%' ESCAPE '\', 'ab' LIKE '%a\%b%' ESCAPE '\'")
t('escape underscore', r"SELECT 'a_b' LIKE '%a\_b%' ESCAPE '\', 'axb' LIKE '%a\_b%' ESCAPE '\'")
t('escape backslash', r"SELECT 'a\b' LIKE '%a\\b%' ESCAPE '\', 'ab' LIKE '%a\\b%' ESCAPE '\'")
