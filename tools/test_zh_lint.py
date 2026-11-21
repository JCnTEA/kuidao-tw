#!/usr/bin/env python3
"""中文自檢規則的單元測試：python tools/test_zh_lint.py"""
from __future__ import annotations

import unittest

import zh_lint


class ZhLintTest(unittest.TestCase):
    def test_taiwan_terms(self):
        text, notes, _ = zh_lint.clean_span("保存信息到文件夾，支持下拉框")
        self.assertEqual(text, "儲存訊息到資料夾，支援下拉選單")
        self.assertTrue(notes)

    def test_repeated_particles_and_punctuation(self):
        text, _, _ = zh_lint.clean_span("目前現在已經經完成了。。")
        self.assertEqual(text, "目前已經完成了。")

    def test_rust_only_changes_strings(self):
        source = 'let msg = "保存信息"; // 保存信息 不應被改\n'
        text, notes, _ = zh_lint.lint_rust(source)
        self.assertEqual(text, 'let msg = "儲存訊息"; // 保存信息 不應被改\n')
        self.assertTrue(notes)

    def test_rust_raw_string_is_checked(self):
        source = 'let msg = r#"支持保存"#;\n'
        text, _, _ = zh_lint.lint_rust(source)
        self.assertEqual(text, 'let msg = r#"支援儲存"#;\n')

    def test_review_phrase_is_reported(self):
        _, _, review = zh_lint.clean_span("對設定進行了重設的動作")
        self.assertEqual(review, ["進行了重設的動作"])


if __name__ == "__main__":
    unittest.main()
