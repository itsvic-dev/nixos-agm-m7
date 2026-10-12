#ifndef T9_ENGINE_H
#define T9_ENGINE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#define T9_MAX_DIGITS 32
#define T9_MAX_CANDIDATES 32
#define T9_WORD_LEN 64
#define T9_COMMIT_LEN 256

enum t9_key {
	T9_KEY_STAR = 10,
	T9_KEY_POUND,
	T9_KEY_BACK,
	T9_KEY_UP,
	T9_KEY_DOWN,
};

enum t9_mode {
	T9_MODE_T9_SENTENCE,
	T9_MODE_T9_LOWER,
	T9_MODE_TAP_SENTENCE,
	T9_MODE_TAP_LOWER,
	T9_MODE_NUMBER,
	T9_MODE_COUNT,
};

struct t9_lang {
	const char *name;
	const char *file;
	const char *keys[10];
	const char *const *capitalized;
};

extern const struct t9_lang t9_langs[];
extern const int t9_lang_count;

struct t9_entry {
	uint32_t key;
	uint32_t word;
	uint32_t count;
};

struct t9_dict {
	char *pool;
	size_t pool_len, pool_cap;
	struct t9_entry *entries;
	size_t count, cap;
};

struct t9_out {
	char commit[T9_COMMIT_LEN];
	int backspaces;
	bool forward;
	bool toggle_osk;
	bool mode_changed;
};

struct t9 {
	const struct t9_dict *dicts;
	int lang;
	enum t9_mode mode;
	bool sentence_start;
	bool sentence_end;

	char digits[T9_MAX_DIGITS + 1];
	int ndigits;
	int symbol_key;
	int tap_key;

	char cand[T9_MAX_CANDIDATES][T9_WORD_LEN];
	int ncand;
	int selected;
};

/* Loads "word count" lines and skips words that the language's keys cannot type. */
int t9_dict_load(struct t9_dict *dict, const struct t9_lang *lang, FILE *f);
void t9_dict_free(struct t9_dict *dict);

/* dicts has one entry per language in t9_langs. */
void t9_init(struct t9 *t, const struct t9_dict *dicts, int lang);

/* Handles a key press. A long press types the digit itself. */
void t9_key(struct t9 *t, int key, bool long_press, struct t9_out *out);
/* Accepts the pending multi-tap character. */
void t9_timeout(struct t9 *t, struct t9_out *out);
/* Commits the word or character being composed. */
void t9_flush(struct t9 *t, struct t9_out *out);
/* Drops the composition without committing it. */
void t9_reset(struct t9 *t);
void t9_set_sentence_start(struct t9 *t, bool start);

bool t9_composing(const struct t9 *t);
bool t9_tap_pending(const struct t9 *t);
const char *t9_preedit(const struct t9 *t);
void t9_mode_name(const struct t9 *t, char *buf, size_t len);

#endif
