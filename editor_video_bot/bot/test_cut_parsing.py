import unittest
from handlers.cut import parse_bool_flag, parse_cut_args


class TestCutParsing(unittest.TestCase):
    def test_parse_bool_flag_truthy(self):
        for val in ["yes", "y", "true", "True", "YES", "да", "ДА", "д", "1", "+"]:
            self.assertTrue(parse_bool_flag(val), f"Failed for {val}")

    def test_parse_bool_flag_falsy(self):
        for val in ["no", "n", "false", "False", "NO", "нет", "НЕТ", "н", "0", "-"]:
            self.assertFalse(parse_bool_flag(val), f"Failed for {val}")

    def test_parse_bool_flag_defaults(self):
        self.assertTrue(parse_bool_flag(None, default=True))
        self.assertFalse(parse_bool_flag(None, default=False))
        self.assertTrue(parse_bool_flag("unknown_value", default=True))

    def test_parse_cut_args_url_only(self):
        url, include_intro = parse_cut_args("/cut_reddit https://youtu.be/xyz123")
        self.assertEqual(url, "https://youtu.be/xyz123")
        self.assertTrue(include_intro)

    def test_parse_cut_args_with_no_flag(self):
        cases = [
            "/cut_reddit https://youtu.be/xyz123 no",
            "/cut https://youtu.be/xyz123 false",
            "/reddit_cut https://youtu.be/xyz123 нет",
            "/cut_reddit https://youtu.be/xyz123 0",
            "/cut_reddit no https://youtu.be/xyz123",
        ]
        for cmd in cases:
            url, include_intro = parse_cut_args(cmd)
            self.assertEqual(url, "https://youtu.be/xyz123", f"Failed url for {cmd}")
            self.assertFalse(include_intro, f"Failed include_intro for {cmd}")

    def test_parse_cut_args_with_yes_flag(self):
        cases = [
            "/cut_reddit https://youtu.be/xyz123 yes",
            "/cut https://youtu.be/xyz123 true",
            "/reddit_cut https://youtu.be/xyz123 да",
            "/cut_reddit https://youtu.be/xyz123 1",
            "/cut_reddit yes https://youtu.be/xyz123",
        ]
        for cmd in cases:
            url, include_intro = parse_cut_args(cmd)
            self.assertEqual(url, "https://youtu.be/xyz123", f"Failed url for {cmd}")
            self.assertTrue(include_intro, f"Failed include_intro for {cmd}")

    def test_parse_cut_args_no_url(self):
        url, include_intro = parse_cut_args("/cut_reddit")
        self.assertIsNone(url)
        self.assertTrue(include_intro)

        url, include_intro = parse_cut_args("")
        self.assertIsNone(url)
        self.assertTrue(include_intro)


if __name__ == "__main__":
    unittest.main()
