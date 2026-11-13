#include "tree_sitter/parser.h"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

enum TokenType { BRACED_FIELD_NAME, BODY_OPEN, RAW_OPEN, MARKDOWN_LINE, BODY_CLOSE };

typedef struct {
    uint32_t field_column;
    bool raw;
} Scanner;

static void advance(TSLexer *lexer) {
    lexer->advance(lexer, false);
}

static bool horizontal_space(int32_t c) {
    return c == ' ' || c == '\t' || c == '\r';
}

static bool name_character(int32_t c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
           (c >= '0' && c <= '9') || c == '_';
}

static bool scan_field(Scanner *scanner, TSLexer *lexer) {
    uint32_t column = lexer->get_column(lexer);
    char name[8] = {0};
    unsigned length = 0;
    while (name_character(lexer->lookahead)) {
        if (length < sizeof(name)) {
            name[length] = (char)lexer->lookahead;
        }
        length++;
        advance(lexer);
    }
    if (length == 0 || (length == 8 && memcmp(name, "evidence", 8) == 0)) {
        return false;
    }

    lexer->mark_end(lexer);
    while (horizontal_space(lexer->lookahead)) {
        advance(lexer);
    }

    if (length == 5 && memcmp(name, "field", 5) == 0 && lexer->lookahead == '"') {
        advance(lexer);
        while (!lexer->eof(lexer) && lexer->lookahead != '"') {
            if (lexer->lookahead == '\\') {
                advance(lexer);
                if (lexer->eof(lexer)) {
                    return false;
                }
            }
            advance(lexer);
        }
        if (lexer->eof(lexer)) {
            return false;
        }
        advance(lexer);
        lexer->mark_end(lexer);
        while (horizontal_space(lexer->lookahead)) {
            advance(lexer);
        }
    }

    if (lexer->lookahead != '{') {
        return false;
    }
    advance(lexer);
    while (horizontal_space(lexer->lookahead)) {
        advance(lexer);
    }
    if (lexer->lookahead != '\n') {
        return false;
    }

    scanner->field_column = column;
    lexer->result_symbol = BRACED_FIELD_NAME;
    return true;
}

static bool triple_quote(TSLexer *lexer) {
    for (unsigned index = 0; index < 3; index++) {
        if (lexer->lookahead != '"') {
            return false;
        }
        advance(lexer);
    }
    return true;
}

static bool scan_open(Scanner *scanner, TSLexer *lexer, bool raw) {
    if (raw) {
        uint32_t column = lexer->get_column(lexer);
        if (!triple_quote(lexer)) {
            return false;
        }
        scanner->field_column = column;
    } else {
        if (lexer->lookahead != '{') {
            return false;
        }
        advance(lexer);
    }

    lexer->mark_end(lexer);
    while (horizontal_space(lexer->lookahead)) {
        advance(lexer);
    }
    if (lexer->lookahead == '\n') {
        advance(lexer);
        lexer->mark_end(lexer);
    } else if (!raw) {
        return false;
    }

    scanner->raw = raw;
    lexer->result_symbol = raw ? RAW_OPEN : BODY_OPEN;
    return true;
}

static bool scan_markdown(const Scanner *scanner, TSLexer *lexer, const bool *valid) {
    const bool line_start = lexer->get_column(lexer) == 0;
    const uint32_t indent = scanner->raw ? scanner->field_column : scanner->field_column + 4;

    // Omit the VDL indentation from each injected Markdown range.
    if (line_start) {
        while (lexer->get_column(lexer) < indent &&
               (lexer->lookahead == ' ' || lexer->lookahead == '\t')) {
            lexer->advance(lexer, true);
        }
    }

    if (!scanner->raw && line_start && lexer->lookahead == '}' &&
        lexer->get_column(lexer) == scanner->field_column && valid[BODY_CLOSE]) {
        advance(lexer);
        lexer->mark_end(lexer);
        if (lexer->lookahead == ',') {
            advance(lexer);
        }
        while (horizontal_space(lexer->lookahead)) {
            advance(lexer);
        }
        if (lexer->lookahead == '\n' || lexer->eof(lexer)) {
            lexer->result_symbol = BODY_CLOSE;
            return true;
        }
    }

    if (scanner->raw && lexer->lookahead == '"' && valid[BODY_CLOSE]) {
        if (triple_quote(lexer)) {
            lexer->mark_end(lexer);
            lexer->result_symbol = BODY_CLOSE;
            return true;
        }
        lexer->mark_end(lexer);
    }

    if (!valid[MARKDOWN_LINE] || lexer->eof(lexer)) {
        return false;
    }
    while (!lexer->eof(lexer)) {
        if (scanner->raw && lexer->lookahead == '"') {
            lexer->mark_end(lexer);
            if (triple_quote(lexer)) {
                lexer->result_symbol = MARKDOWN_LINE;
                return true;
            }
            continue;
        }
        bool newline = lexer->lookahead == '\n';
        advance(lexer);
        if (newline) {
            break;
        }
    }
    lexer->mark_end(lexer);
    lexer->result_symbol = MARKDOWN_LINE;
    return true;
}

void *tree_sitter_vdl_external_scanner_create(void) {
    return calloc(1, sizeof(Scanner));
}

void tree_sitter_vdl_external_scanner_destroy(void *payload) {
    free(payload);
}

unsigned tree_sitter_vdl_external_scanner_serialize(void *payload, char *buffer) {
    const Scanner *scanner = payload;
    for (unsigned byte = 0; byte < 4; byte++) {
        buffer[byte] = (char)(scanner->field_column >> (byte * 8));
    }
    buffer[4] = scanner->raw;
    return 5;
}

void tree_sitter_vdl_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    Scanner *scanner = payload;
    scanner->field_column = 0;
    scanner->raw = length == 5 && buffer[4];
    if (length == 5) {
        for (unsigned byte = 0; byte < 4; byte++) {
            scanner->field_column |= (uint32_t)(uint8_t)buffer[byte] << (byte * 8);
        }
    }
}

bool tree_sitter_vdl_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid) {
    if (valid[BRACED_FIELD_NAME] && valid[MARKDOWN_LINE]) {
        return false;
    }
    if (valid[MARKDOWN_LINE] || valid[BODY_CLOSE]) {
        return scan_markdown(payload, lexer, valid);
    }
    while (horizontal_space(lexer->lookahead) || lexer->lookahead == '\n') {
        lexer->advance(lexer, true);
    }
    if (valid[BODY_OPEN]) {
        return scan_open(payload, lexer, false);
    }
    if (valid[RAW_OPEN] && lexer->lookahead == '"') {
        return scan_open(payload, lexer, true);
    }
    return valid[BRACED_FIELD_NAME] && scan_field(payload, lexer);
}
