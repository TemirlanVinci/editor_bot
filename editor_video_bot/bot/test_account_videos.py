import os
import shutil
import tempfile
import unittest
from unittest.mock import patch

from handlers.accounts import (
    count_account_videos,
    get_account_videos_details,
    resolve_account_info,
    check_and_report_account_videos,
    ORDINAL_MAP,
)


class TestAccountVideos(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_ordinals(self):
        self.assertEqual(ORDINAL_MAP["1"], 1)
        self.assertEqual(ORDINAL_MAP["первый"], 1)
        self.assertEqual(ORDINAL_MAP["1-й"], 1)
        self.assertEqual(ORDINAL_MAP["второй"], 2)
        self.assertEqual(ORDINAL_MAP["2-й"], 2)
        self.assertEqual(ORDINAL_MAP["третий"], 3)

    def test_resolve_account_info_with_accounts_list(self):
        accounts = [
            {"id": 10, "name": "MainTikTok"},
            {"id": 20, "name": "SecondChannel"},
        ]

        # 1st account by ordinal
        acc_id, acc, idx = resolve_account_info("первый", accounts)
        self.assertEqual(acc_id, 10)
        self.assertEqual(acc["name"], "MainTikTok")
        self.assertEqual(idx, 1)

        # 2nd account by ordinal
        acc_id, acc, idx = resolve_account_info("второй", accounts)
        self.assertEqual(acc_id, 20)
        self.assertEqual(acc["name"], "SecondChannel")
        self.assertEqual(idx, 2)

        # By exact ID
        acc_id, acc, idx = resolve_account_info("10", accounts)
        self.assertEqual(acc_id, 10)
        self.assertEqual(idx, 1)

        # By folder name
        acc_id, acc, idx = resolve_account_info("acc_20", accounts)
        self.assertEqual(acc_id, 20)
        self.assertEqual(idx, 2)

        # By account name
        acc_id, acc, idx = resolve_account_info("mainTIKTOK", accounts)
        self.assertEqual(acc_id, 10)
        self.assertEqual(idx, 1)

    def test_count_and_details_empty(self):
        with patch("handlers.accounts.MEDIA_DIR", self.test_dir):
            details = get_account_videos_details(1)
            self.assertFalse(details["exists"])
            self.assertEqual(details["count"], 0)
            self.assertEqual(count_account_videos(1), 0)

    def test_count_and_details_with_files(self):
        with patch("handlers.accounts.MEDIA_DIR", self.test_dir):
            acc_dir = os.path.join(self.test_dir, "acc_1")
            os.makedirs(acc_dir, exist_ok=True)

            # Create video files and non-video files
            for fname in ["clip1.mp4", "clip2.mov", "clip3.mkv", "ignore.txt", ".hidden.mp4"]:
                with open(os.path.join(acc_dir, fname), "w") as f:
                    f.write("test content")

            details = get_account_videos_details(1)
            self.assertTrue(details["exists"])
            self.assertEqual(details["count"], 3)
            self.assertEqual(count_account_videos(1), 3)

            filenames = [f["name"] for f in details["files"]]
            self.assertIn("clip1.mp4", filenames)
            self.assertIn("clip2.mov", filenames)
            self.assertIn("clip3.mkv", filenames)
            self.assertNotIn("ignore.txt", filenames)
            self.assertNotIn(".hidden.mp4", filenames)

    def test_check_and_report_account_videos(self):
        with patch("handlers.accounts.MEDIA_DIR", self.test_dir):
            acc_dir = os.path.join(self.test_dir, "acc_5")
            os.makedirs(acc_dir, exist_ok=True)
            with open(os.path.join(acc_dir, "vid.mp4"), "w") as f:
                f.write("content")

            accounts = [{"id": 5, "name": "TestBot", "publish_time": "12:00,18:00"}]
            report = check_and_report_account_videos("первый", accounts=accounts, print_output=False)

            self.assertEqual(report["account_id"], 5)
            self.assertEqual(report["account_name"], "TestBot")
            self.assertEqual(report["count"], 1)
            self.assertIn("TestBot", report["message_text"])
            self.assertIn("`vid.mp4`", report["message_text"])
            self.assertIn("12:00,18:00", report["message_text"])

    def test_keyboard_with_counts(self):
        from keyboards.acc_kb import get_account_videos_keyboard
        accounts = [
            {"id": 1, "name": "Alpha"},
            {"id": 2, "name": "Beta"},
        ]
        video_counts = {1: 7, 2: 0}
        kb = get_account_videos_keyboard(accounts, video_counts=video_counts)

        btn1_text = kb.inline_keyboard[0][0].text
        btn2_text = kb.inline_keyboard[1][0].text

        self.assertIn("Alpha (7 видео)", btn1_text)
        self.assertIn("Beta (0 видео)", btn2_text)
        self.assertEqual(kb.inline_keyboard[0][0].callback_data, "acc_videos:view:1")
        self.assertEqual(kb.inline_keyboard[1][0].callback_data, "acc_videos:view:2")


if __name__ == "__main__":
    unittest.main()

