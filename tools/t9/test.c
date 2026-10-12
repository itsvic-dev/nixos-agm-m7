#include "engine.h"

#include <stdlib.h>
#include <string.h>

enum { EN, PL };

static int failures;
static struct t9_dict dicts[2];
static struct t9 t;
static struct t9_out out;

#define CHECK_STR(actual, expected)                                                                   \
	do {                                                                                          \
		if (strcmp((actual), (expected))) {                                                   \
			fprintf(stderr, "  %s:%d: got \"%s\", expected \"%s\"\n", __FILE__, __LINE__, \
				(actual), (expected));                                                \
			failures++;                                                                   \
		}                                                                                     \
	} while (0)

#define CHECK(cond)                                                                \
	do {                                                                       \
		if (!(cond)) {                                                     \
			fprintf(stderr, "  %s:%d: %s\n", __FILE__, __LINE__, #cond); \
			failures++;                                                \
		}                                                                  \
	} while (0)

static void setup(int lang, const char *en, const char *pl)
{
	const char *words[2] = { en, pl };

	for (int i = 0; i < 2; i++) {
		FILE *f = tmpfile();

		t9_dict_free(&dicts[i]);
		if (!f || fputs(words[i], f) < 0 || fseek(f, 0, SEEK_SET) ||
		    t9_dict_load(&dicts[i], &t9_langs[i], f)) {
			perror("t9_dict_load");
			exit(1);
		}
		fclose(f);
	}
	t9_init(&t, dicts, lang);
}

static void press(int key)
{
	t9_key(&t, key, false, &out);
}

static void type(const char *keys)
{
	for (; *keys; keys++) {
		if (*keys == '*')
			press(T9_KEY_STAR);
		else if (*keys == '#')
			press(T9_KEY_POUND);
		else
			press(*keys - '0');
	}
}

static void use_mode(enum t9_mode mode)
{
	while (t.mode != mode)
		press(T9_KEY_POUND);
}

static void shows_the_most_frequent_word_first(void)
{
	setup(EN, "good 100\nhome 50\ngone 20\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4663");
	CHECK_STR(t9_preedit(&t), "good");
}

static void star_steps_to_the_next_word(void)
{
	setup(EN, "good 100\nhome 50\ngone 20\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4663**");
	CHECK_STR(t9_preedit(&t), "gone");
}

static void zero_commits_the_word_and_a_space(void)
{
	setup(EN, "good 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4663");
	press(0);
	CHECK_STR(out.commit, "good ");
	CHECK(!t9_composing(&t));
}

static void capitalizes_the_first_word_of_a_sentence(void)
{
	setup(EN, "good 100\nday 50\n", "");
	type("4663");
	CHECK_STR(t9_preedit(&t), "Good");
	press(0);
	type("329");
	CHECK_STR(t9_preedit(&t), "day");
}

static void capitalizes_after_a_full_stop_and_a_space(void)
{
	setup(EN, "good 100\nday 50\n", "");
	type("4663010");
	type("329");
	CHECK_STR(t9_preedit(&t), "Day");
}

static void lowercase_mode_does_not_capitalize(void)
{
	setup(EN, "good 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4663");
	CHECK_STR(t9_preedit(&t), "good");
}

static void always_capitalizes_english_i(void)
{
	setup(EN, "i 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4");
	CHECK_STR(t9_preedit(&t), "I");
}

static void shows_a_word_prefix_while_typing(void)
{
	setup(EN, "good 100\nhome 50\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("46");
	CHECK_STR(t9_preedit(&t), "go");
}

static void falls_back_to_letters_for_unknown_words(void)
{
	setup(EN, "go 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("468");
	CHECK_STR(t9_preedit(&t), "got");
}

static void types_letters_without_a_dictionary(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_T9_LOWER);
	type("23");
	CHECK_STR(t9_preedit(&t), "ad");
}

static void key_one_types_an_apostrophe_inside_a_known_word(void)
{
	setup(EN, "don't 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("36618");
	CHECK_STR(t9_preedit(&t), "don't");
}

static void key_one_after_a_word_adds_punctuation(void)
{
	setup(EN, "good 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("46631");
	CHECK_STR(out.commit, "");
	CHECK_STR(t9_preedit(&t), "good.");
}

static void key_one_again_steps_the_punctuation(void)
{
	setup(EN, "good 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("466311");
	CHECK_STR(t9_preedit(&t), "good,");
}

static void punctuation_comes_before_dictionary_words(void)
{
	setup(EN, "the 100\nthe-end 1\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("8431");
	CHECK_STR(t9_preedit(&t), "the.");
}

static void a_digit_after_punctuation_starts_a_new_word(void)
{
	setup(EN, "good 100\nday 50\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("466313");
	CHECK_STR(out.commit, "good.");
	type("29");
	CHECK_STR(t9_preedit(&t), "day");
}

static void key_one_alone_starts_punctuation(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_T9_LOWER);
	type("11");
	CHECK_STR(t9_preedit(&t), ",");
}

static void back_removes_the_last_digit(void)
{
	setup(EN, "good 100\ngo 50\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4663");
	press(T9_KEY_BACK);
	press(T9_KEY_BACK);
	CHECK_STR(t9_preedit(&t), "go");
}

static void back_without_a_word_deletes_a_character(void)
{
	setup(EN, "", "");
	press(T9_KEY_BACK);
	CHECK(out.backspaces == 1);
}

static void long_back_toggles_the_keyboard(void)
{
	setup(EN, "", "");
	t9_key(&t, T9_KEY_BACK, true, &out);
	CHECK(out.toggle_osk);
	CHECK(out.backspaces == 0);
}

static void long_press_types_the_digit(void)
{
	setup(EN, "good 100\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("466");
	t9_key(&t, 3, true, &out);
	CHECK_STR(out.commit, "goo3");
}

static void arrows_select_candidates_while_composing(void)
{
	setup(EN, "good 100\nhome 50\n", "");
	use_mode(T9_MODE_T9_LOWER);
	type("4663");
	press(T9_KEY_DOWN);
	CHECK_STR(t9_preedit(&t), "home");
	CHECK(!out.forward);
	press(T9_KEY_UP);
	CHECK_STR(t9_preedit(&t), "good");
}

static void arrows_pass_through_when_idle(void)
{
	setup(EN, "", "");
	press(T9_KEY_DOWN);
	CHECK(out.forward);
}

static void star_opens_symbols_when_idle(void)
{
	setup(EN, "", "");
	press(T9_KEY_STAR);
	CHECK(t9_composing(&t));
	CHECK_STR(t9_preedit(&t), ".");
}

static void multi_tap_cycles_letters_on_one_key(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_TAP_LOWER);
	type("22");
	CHECK_STR(t9_preedit(&t), "b");
	CHECK(t9_tap_pending(&t));
}

static void multi_tap_commits_on_timeout(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_TAP_LOWER);
	type("222");
	t9_timeout(&t, &out);
	CHECK_STR(out.commit, "c");
	CHECK(!t9_composing(&t));
}

static void multi_tap_commits_when_another_key_is_pressed(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_TAP_LOWER);
	type("223");
	CHECK_STR(out.commit, "b");
	CHECK_STR(t9_preedit(&t), "d");
}

static void multi_tap_capitalizes_only_the_first_letter(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_TAP_SENTENCE);
	type("44");
	t9_timeout(&t, &out);
	type("8");
	t9_timeout(&t, &out);
	CHECK_STR(out.commit, "t");
}

static void number_mode_types_digits(void)
{
	setup(EN, "", "");
	use_mode(T9_MODE_NUMBER);
	press(4);
	CHECK_STR(out.commit, "4");
}

static void pound_cycles_the_modes(void)
{
	char name[32];

	setup(EN, "", "");
	press(T9_KEY_POUND);
	CHECK(out.mode_changed);
	t9_mode_name(&t, name, sizeof(name));
	CHECK_STR(name, "T9 abc EN");
}

static void long_pound_switches_the_language(void)
{
	char name[32];

	setup(EN, "", "");
	t9_key(&t, T9_KEY_POUND, true, &out);
	t9_mode_name(&t, name, sizeof(name));
	CHECK_STR(name, "T9 Abc PL");
}

static void polish_words_use_the_polish_keys(void)
{
	setup(PL, "", "łódź 10\n");
	type("5639");
	CHECK_STR(t9_preedit(&t), "Łódź");
}

static void polish_multi_tap_has_diacritics(void)
{
	setup(PL, "", "");
	use_mode(T9_MODE_TAP_LOWER);
	type("2222");
	CHECK_STR(t9_preedit(&t), "ą");
}

static void dictionary_skips_words_the_keys_cannot_type(void)
{
	setup(EN, "2pac 100\ncafé 50\nmr. 20\nthe- 20\nbad 10\n", "");
	use_mode(T9_MODE_T9_LOWER);
	CHECK(dicts[EN].count == 1);
}

int main(void)
{
	static const struct {
		const char *name;
		void (*fn)(void);
	} tests[] = {
#define T(fn) { #fn, fn }
		T(shows_the_most_frequent_word_first),
		T(star_steps_to_the_next_word),
		T(zero_commits_the_word_and_a_space),
		T(capitalizes_the_first_word_of_a_sentence),
		T(capitalizes_after_a_full_stop_and_a_space),
		T(lowercase_mode_does_not_capitalize),
		T(always_capitalizes_english_i),
		T(shows_a_word_prefix_while_typing),
		T(falls_back_to_letters_for_unknown_words),
		T(types_letters_without_a_dictionary),
		T(key_one_types_an_apostrophe_inside_a_known_word),
		T(key_one_after_a_word_adds_punctuation),
		T(key_one_again_steps_the_punctuation),
		T(punctuation_comes_before_dictionary_words),
		T(a_digit_after_punctuation_starts_a_new_word),
		T(key_one_alone_starts_punctuation),
		T(back_removes_the_last_digit),
		T(back_without_a_word_deletes_a_character),
		T(long_back_toggles_the_keyboard),
		T(long_press_types_the_digit),
		T(arrows_select_candidates_while_composing),
		T(arrows_pass_through_when_idle),
		T(star_opens_symbols_when_idle),
		T(multi_tap_cycles_letters_on_one_key),
		T(multi_tap_commits_on_timeout),
		T(multi_tap_commits_when_another_key_is_pressed),
		T(multi_tap_capitalizes_only_the_first_letter),
		T(number_mode_types_digits),
		T(pound_cycles_the_modes),
		T(long_pound_switches_the_language),
		T(polish_words_use_the_polish_keys),
		T(polish_multi_tap_has_diacritics),
		T(dictionary_skips_words_the_keys_cannot_type),
#undef T
	};
	int failed = 0;

	for (size_t i = 0; i < sizeof(tests) / sizeof(tests[0]); i++) {
		int before = failures;

		tests[i].fn();
		if (failures != before) {
			fprintf(stderr, "FAIL %s\n", tests[i].name);
			failed++;
		}
	}
	printf("%zu tests, %d failed\n", sizeof(tests) / sizeof(tests[0]), failed);
	return failed != 0;
}
