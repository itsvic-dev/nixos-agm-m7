#include "engine.h"

#include <ctype.h>
#include <stdlib.h>
#include <string.h>

#define PREFIX_CANDIDATES 8

static const char *const en_capitalized[] = { "i", "i'm", "i'll", "i've", "i'd", NULL };

const struct t9_lang t9_langs[] = {
	{
		.name = "EN",
		.file = "en.txt",
		.keys = { " 0", ".,?!'-:;@/()1", "abc2", "def3", "ghi4", "jkl5", "mno6", "pqrs7", "tuv8", "wxyz9" },
		.capitalized = en_capitalized,
	},
	{
		.name = "PL",
		.file = "pl.txt",
		.keys = { " 0", ".,?!'-:;@/()1", "abcąć2", "defę3", "ghi4", "jklł5", "mnońó6", "pqrsś7", "tuv8",
			  "wxyzźż9" },
	},
};
const int t9_lang_count = sizeof(t9_langs) / sizeof(t9_langs[0]);

static const char star_symbols[] = ".,?!'\"-()@/:;_+&%*=<>$€#[]{}\\~^|";

static const char *const mode_names[T9_MODE_COUNT] = { "T9 Abc", "T9 abc", "Abc", "abc", "123" };

/* Polish capitals have the same UTF-8 length as their small letters. */
static const char *const upper_map[][2] = {
	{ "ą", "Ą" }, { "ć", "Ć" }, { "ę", "Ę" }, { "ł", "Ł" }, { "ń", "Ń" },
	{ "ó", "Ó" }, { "ś", "Ś" }, { "ź", "Ź" }, { "ż", "Ż" },
};

static int utf8_len(const char *s)
{
	unsigned char c = *s;

	if (c < 0x80)
		return 1;
	if ((c & 0xe0) == 0xc0)
		return 2;
	if ((c & 0xf0) == 0xe0)
		return 3;
	return 4;
}

static int key_of(const struct t9_lang *lang, const char *ch, int len)
{
	for (int k = 0; k < 10; k++) {
		for (const char *p = lang->keys[k]; *p; p += utf8_len(p)) {
			if (utf8_len(p) == len && !memcmp(p, ch, len))
				return k;
		}
	}
	return -1;
}

static bool word_key(const struct t9_lang *lang, const char *word, char *key)
{
	int n = 0;

	for (const char *p = word; *p; p += utf8_len(p)) {
		int len = utf8_len(p);
		int k;

		if (isdigit((unsigned char)*p) || n == T9_MAX_DIGITS)
			return false;
		k = key_of(lang, p, len);
		if (k <= 0 || (k == 1 && *p != '\'' && *p != '-'))
			return false;
		key[n++] = '0' + k;
	}
	key[n] = 0;
	return n > 0 && key[n - 1] != '1';
}

static void capitalize(char *s)
{
	if (islower((unsigned char)*s)) {
		*s = toupper((unsigned char)*s);
		return;
	}
	for (size_t i = 0; i < sizeof(upper_map) / sizeof(upper_map[0]); i++) {
		if (!strncmp(s, upper_map[i][0], strlen(upper_map[i][0]))) {
			memcpy(s, upper_map[i][1], strlen(upper_map[i][1]));
			return;
		}
	}
}

static uint32_t pool_add(struct t9_dict *d, const char *s)
{
	size_t len = strlen(s) + 1;
	uint32_t off = d->pool_len;

	if (d->pool_len + len > d->pool_cap) {
		size_t cap = d->pool_cap ? d->pool_cap * 2 : 65536;
		char *pool;

		while (cap < d->pool_len + len)
			cap *= 2;
		pool = realloc(d->pool, cap);
		if (!pool)
			return UINT32_MAX;
		d->pool = pool;
		d->pool_cap = cap;
	}
	memcpy(d->pool + d->pool_len, s, len);
	d->pool_len += len;
	return off;
}

static const char *sort_pool;

static int entry_cmp(const void *a, const void *b)
{
	const struct t9_entry *x = a, *y = b;
	int c = strcmp(sort_pool + x->key, sort_pool + y->key);

	if (c)
		return c;
	if (x->count != y->count)
		return x->count > y->count ? -1 : 1;
	return x->word < y->word ? -1 : x->word > y->word;
}

int t9_dict_load(struct t9_dict *d, const struct t9_lang *lang, FILE *f)
{
	char *line = NULL;
	size_t cap = 0;
	ssize_t n;

	while ((n = getline(&line, &cap, f)) > 0) {
		char key[T9_MAX_DIGITS + 1];
		struct t9_entry e;
		char *sp;

		line[strcspn(line, "\r\n")] = 0;
		sp = strrchr(line, ' ');
		if (!sp)
			continue;
		*sp = 0;
		if (strlen(line) >= T9_WORD_LEN || !word_key(lang, line, key))
			continue;

		if (d->count == d->cap) {
			size_t ncap = d->cap ? d->cap * 2 : 4096;
			struct t9_entry *entries = realloc(d->entries, ncap * sizeof(*entries));

			if (!entries)
				goto oom;
			d->entries = entries;
			d->cap = ncap;
		}
		e.count = strtoul(sp + 1, NULL, 10);
		e.key = pool_add(d, key);
		e.word = pool_add(d, line);
		if (e.key == UINT32_MAX || e.word == UINT32_MAX)
			goto oom;
		d->entries[d->count++] = e;
	}
	free(line);

	sort_pool = d->pool;
	qsort(d->entries, d->count, sizeof(*d->entries), entry_cmp);
	return 0;

oom:
	free(line);
	return -1;
}

void t9_dict_free(struct t9_dict *d)
{
	free(d->pool);
	free(d->entries);
	memset(d, 0, sizeof(*d));
}

static const char *entry_key(const struct t9_dict *d, size_t i)
{
	return d->pool + d->entries[i].key;
}

static size_t lower_bound(const struct t9_dict *d, const char *digits)
{
	size_t lo = 0, hi = d->count;

	while (lo < hi) {
		size_t mid = lo + (hi - lo) / 2;

		if (strcmp(entry_key(d, mid), digits) < 0)
			lo = mid + 1;
		else
			hi = mid;
	}
	return lo;
}

static bool has_prefix(const struct t9 *t, const char *digits)
{
	const struct t9_dict *d = &t->dicts[t->lang];
	size_t i = lower_bound(d, digits);

	return i < d->count && !strncmp(entry_key(d, i), digits, strlen(digits));
}

static void copy_chars(char *dst, const char *src, int nchars)
{
	const char *p = src;

	for (int i = 0; i < nchars && *p; i++)
		p += utf8_len(p);
	memcpy(dst, src, p - src);
	dst[p - src] = 0;
}

static int exact_words(const struct t9_dict *d, size_t first, const char *digits, char cand[][T9_WORD_LEN])
{
	int nc = 0;

	for (size_t i = first; i < d->count && nc < T9_MAX_CANDIDATES; i++) {
		if (strcmp(entry_key(d, i), digits))
			break;
		strcpy(cand[nc++], d->pool + d->entries[i].word);
	}
	return nc;
}

static int prefix_words(const struct t9_dict *d, size_t first, const char *digits, int max,
			char cand[][T9_WORD_LEN])
{
	uint32_t counts[PREFIX_CANDIDATES];
	int n = strlen(digits), nc = 0;

	if (max > PREFIX_CANDIDATES)
		max = PREFIX_CANDIDATES;
	for (size_t i = first; i < d->count; i++) {
		const struct t9_entry *e = &d->entries[i];
		char prefix[T9_WORD_LEN];
		int j, min = 0;

		if (strncmp(entry_key(d, i), digits, n))
			break;
		copy_chars(prefix, d->pool + e->word, n);
		for (j = 0; j < nc && strcmp(cand[j], prefix); j++)
			;
		if (j < nc) {
			if (e->count > counts[j])
				counts[j] = e->count;
			continue;
		}
		if (nc < max) {
			j = nc++;
		} else {
			for (j = 1; j < nc; j++) {
				if (counts[j] < counts[min])
					min = j;
			}
			if (e->count <= counts[min])
				continue;
			j = min;
		}
		strcpy(cand[j], prefix);
		counts[j] = e->count;
	}

	for (int a = 1; a < nc; a++) {
		for (int b = a; b > 0 && counts[b] > counts[b - 1]; b--) {
			char tmp[T9_WORD_LEN];
			uint32_t c = counts[b];

			counts[b] = counts[b - 1];
			counts[b - 1] = c;
			strcpy(tmp, cand[b]);
			strcpy(cand[b], cand[b - 1]);
			strcpy(cand[b - 1], tmp);
		}
	}
	return nc;
}

static int find_words(const struct t9 *t, int n, char cand[][T9_WORD_LEN])
{
	const struct t9_dict *d = &t->dicts[t->lang];
	const char *letters = t9_langs[t->lang].keys[t->digits[n - 1] - '0'];
	char digits[T9_MAX_DIGITS + 1];
	size_t first;
	int nc;

	memcpy(digits, t->digits, n);
	digits[n] = 0;
	first = lower_bound(d, digits);

	nc = exact_words(d, first, digits, cand);
	if (nc)
		return nc;

	if (n > 1 && digits[n - 1] == '1') {
		char base[T9_WORD_LEN];

		find_words(t, n - 1, cand);
		strcpy(base, cand[0]);
		for (const char *p = letters; *p && nc < T9_MAX_CANDIDATES; p += utf8_len(p)) {
			if (isdigit((unsigned char)*p) || strlen(base) + utf8_len(p) >= T9_WORD_LEN)
				continue;
			strcpy(cand[nc], base);
			copy_chars(cand[nc] + strlen(base), p, 1);
			nc++;
		}
		return nc + prefix_words(d, first, digits, T9_MAX_CANDIDATES - nc, cand + nc);
	}

	nc = prefix_words(d, first, digits, T9_MAX_CANDIDATES, cand);
	if (nc)
		return nc;

	if (n > 1)
		find_words(t, n - 1, cand);
	else
		cand[0][0] = 0;
	if (strlen(cand[0]) + utf8_len(letters) < T9_WORD_LEN)
		copy_chars(cand[0] + strlen(cand[0]), letters, 1);
	return 1;
}

static bool sentence_case(const struct t9 *t)
{
	return (t->mode == T9_MODE_T9_SENTENCE || t->mode == T9_MODE_TAP_SENTENCE) && t->sentence_start;
}

static void apply_case(const struct t9 *t, char *word)
{
	const char *const *cap = t9_langs[t->lang].capitalized;

	for (; cap && *cap; cap++) {
		if (!strcmp(*cap, word)) {
			capitalize(word);
			return;
		}
	}
	if (sentence_case(t))
		capitalize(word);
}

static void refresh(struct t9 *t)
{
	t->ncand = find_words(t, t->ndigits, t->cand);
	t->selected = 0;
	for (int i = 0; i < t->ncand; i++)
		apply_case(t, t->cand[i]);
}

static void fill_chars(struct t9 *t, const char *list, bool keep_digits)
{
	t->ncand = 0;
	t->selected = 0;
	for (const char *p = list; *p && t->ncand < T9_MAX_CANDIDATES; p += utf8_len(p)) {
		if (!keep_digits && isdigit((unsigned char)*p))
			continue;
		copy_chars(t->cand[t->ncand], p, 1);
		if (sentence_case(t))
			capitalize(t->cand[t->ncand]);
		t->ncand++;
	}
}

static void emit(struct t9 *t, struct t9_out *out, const char *s)
{
	size_t len = strlen(out->commit);

	snprintf(out->commit + len, sizeof(out->commit) - len, "%s", s);
	for (; *s; s++) {
		if (*s == '.' || *s == '?' || *s == '!') {
			t->sentence_start = false;
			t->sentence_end = true;
		} else if (*s == ' ') {
			if (t->sentence_end)
				t->sentence_start = true;
			t->sentence_end = false;
		} else if (*s == '\n') {
			t->sentence_start = true;
			t->sentence_end = false;
		} else {
			t->sentence_start = false;
			t->sentence_end = false;
		}
	}
}

void t9_init(struct t9 *t, const struct t9_dict *dicts, int lang)
{
	memset(t, 0, sizeof(*t));
	t->dicts = dicts;
	t->lang = lang;
	t->mode = T9_MODE_T9_SENTENCE;
	t->sentence_start = true;
	t9_reset(t);
}

void t9_reset(struct t9 *t)
{
	t->ndigits = 0;
	t->digits[0] = 0;
	t->symbol_key = -1;
	t->tap_key = -1;
	t->ncand = 0;
	t->selected = 0;
}

bool t9_composing(const struct t9 *t)
{
	return t->ndigits > 0 || t->symbol_key >= 0 || t->tap_key >= 0;
}

bool t9_tap_pending(const struct t9 *t)
{
	return t->tap_key >= 0;
}

const char *t9_preedit(const struct t9 *t)
{
	return t9_composing(t) && t->ncand ? t->cand[t->selected] : "";
}

void t9_mode_name(const struct t9 *t, char *buf, size_t len)
{
	snprintf(buf, len, "%s %s", mode_names[t->mode], t9_langs[t->lang].name);
}

void t9_set_sentence_start(struct t9 *t, bool start)
{
	t->sentence_start = start;
	t->sentence_end = false;
}

void t9_flush(struct t9 *t, struct t9_out *out)
{
	if (t9_composing(t) && t->ncand)
		emit(t, out, t->cand[t->selected]);
	t9_reset(t);
}

void t9_timeout(struct t9 *t, struct t9_out *out)
{
	memset(out, 0, sizeof(*out));
	if (t9_tap_pending(t))
		t9_flush(t, out);
}

static void start_symbols(struct t9 *t, struct t9_out *out, int key, const char *list)
{
	t9_flush(t, out);
	fill_chars(t, list, false);
	t->symbol_key = key;
}

static void next_candidate(struct t9 *t, int step)
{
	if (t->ncand)
		t->selected = (t->selected + step + t->ncand) % t->ncand;
}

static void tap(struct t9 *t, int key, struct t9_out *out)
{
	if (t->tap_key == key) {
		next_candidate(t, 1);
		return;
	}
	t9_flush(t, out);
	fill_chars(t, t9_langs[t->lang].keys[key], true);
	t->tap_key = key;
}

static void t9_digit(struct t9 *t, int key, struct t9_out *out)
{
	if (key == 0) {
		t9_flush(t, out);
		emit(t, out, " ");
		return;
	}
	if (t->symbol_key >= 0) {
		if (key == 1 && t->symbol_key == 1) {
			next_candidate(t, 1);
			return;
		}
		t9_flush(t, out);
	}
	if (t->ndigits && t->digits[t->ndigits - 1] == '1') {
		char digits[T9_MAX_DIGITS + 2];

		snprintf(digits, sizeof(digits), "%s%c", t->digits, '0' + key);
		if (!has_prefix(t, digits)) {
			if (key == 1) {
				next_candidate(t, 1);
				return;
			}
			t9_flush(t, out);
		}
	}
	if (t->ndigits == T9_MAX_DIGITS)
		t9_flush(t, out);
	if (key == 1 && !t->ndigits) {
		start_symbols(t, out, 1, t9_langs[t->lang].keys[1]);
		return;
	}
	t->digits[t->ndigits++] = '0' + key;
	t->digits[t->ndigits] = 0;
	refresh(t);
}

void t9_key(struct t9 *t, int key, bool long_press, struct t9_out *out)
{
	memset(out, 0, sizeof(*out));

	switch (key) {
	case T9_KEY_BACK:
		if (long_press) {
			out->toggle_osk = true;
		} else if (t->ndigits > 1) {
			t->digits[--t->ndigits] = 0;
			refresh(t);
		} else if (t9_composing(t)) {
			t9_reset(t);
		} else {
			out->backspaces = 1;
		}
		return;
	case T9_KEY_UP:
	case T9_KEY_DOWN:
		if (t9_composing(t) && t->ncand > 1) {
			next_candidate(t, key == T9_KEY_DOWN ? 1 : -1);
		} else {
			t9_flush(t, out);
			out->forward = true;
		}
		return;
	case T9_KEY_POUND:
		t9_flush(t, out);
		if (long_press)
			t->lang = (t->lang + 1) % t9_lang_count;
		else
			t->mode = (t->mode + 1) % T9_MODE_COUNT;
		out->mode_changed = true;
		return;
	case T9_KEY_STAR:
		if (t9_composing(t) && !t9_tap_pending(t))
			next_candidate(t, 1);
		else
			start_symbols(t, out, T9_KEY_STAR, star_symbols);
		return;
	}

	if (key < 0 || key > 9)
		return;
	if (long_press || t->mode == T9_MODE_NUMBER) {
		char digit[2] = { '0' + key, 0 };

		t9_flush(t, out);
		emit(t, out, digit);
	} else if (t->mode == T9_MODE_TAP_SENTENCE || t->mode == T9_MODE_TAP_LOWER) {
		tap(t, key, out);
	} else {
		t9_digit(t, key, out);
	}
}
