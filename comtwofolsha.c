#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <sys/stat.h>
#include <sys/ioctl.h>
#include <unistd.h>
#include <openssl/evp.h>

#define GREEN   "\033[0;32m"
#define RED     "\033[0;31m"
#define YELLOW  "\033[1;33m"
#define CYAN    "\033[0;36m"
#define NC      "\033[0m"

void print_help(const char* prog) {
    printf(
        "Folder File Comparison Utility\n"
        "Usage: %s [FOLDER1] [FOLDER2]\n"
        "Compares files in FOLDER1 and FOLDER2 by SHA256 hash.\n\n"
        "Options:\n"
        "  -h, --help      Show this help message and exit\n"
        "\n"
        "Example:\n"
        "  %s /path/to/folder1 /path/to/folder2\n",
        prog, prog
    );
}

int get_term_width() {
    struct winsize w;
    if (ioctl(STDOUT_FILENO, TIOCGWINSZ, &w) == 0 && w.ws_col > 0) return w.ws_col;
    char* cols = getenv("COLUMNS");
    if (cols) return atoi(cols);
    return 80;
}

void center(const char* msg, int width) {
    int len = (int)strlen(msg);
    int pad = (width - len) / 2;
    if (pad > 0) printf("%*s%s\n", pad, "", msg);
    else printf("%s\n", msg);
}

char* file_sha256(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) return NULL;

    EVP_MD_CTX* ctx = EVP_MD_CTX_new();
    if (!ctx) { fclose(f); return NULL; }
    const EVP_MD* md = EVP_sha256();
    unsigned char hash[EVP_MAX_MD_SIZE];
    unsigned int hash_len = 0;
    unsigned char buffer[4096];
    size_t n;

    if (!EVP_DigestInit_ex(ctx, md, NULL)) {
        EVP_MD_CTX_free(ctx); fclose(f); return NULL;
    }

    while ((n = fread(buffer, 1, sizeof(buffer), f)) > 0) {
        if (!EVP_DigestUpdate(ctx, buffer, n)) {
            EVP_MD_CTX_free(ctx); fclose(f); return NULL;
        }
    }

    if (!EVP_DigestFinal_ex(ctx, hash, &hash_len)) {
        EVP_MD_CTX_free(ctx); fclose(f); return NULL;
    }

    EVP_MD_CTX_free(ctx);
    fclose(f);

    char* hex = malloc(hash_len * 2 + 1);
    if (!hex) return NULL;
    for (unsigned int i = 0; i < hash_len; ++i)
        sprintf(hex + i * 2, "%02x", hash[i]);
    hex[hash_len * 2] = 0;
    return hex;
}

int compute_max_filename_len(const char* folder1, const char* folder2) {
    int max = 0;
    DIR* d;
    struct dirent* ent;
    char path[1024];
    struct stat st;

    d = opendir(folder1);
    if (d) {
        while ((ent = readdir(d))) {
            if (!strcmp(ent->d_name, ".") || !strcmp(ent->d_name, "..")) continue;
            snprintf(path, sizeof(path), "%s/%s", folder1, ent->d_name);
            if (stat(path, &st) == 0 && !S_ISDIR(st.st_mode)) {
                int l = (int)strlen(ent->d_name);
                if (l > max) max = l;
            }
        }
        closedir(d);
    }

    d = opendir(folder2);
    if (d) {
        while ((ent = readdir(d))) {
            if (!strcmp(ent->d_name, ".") || !strcmp(ent->d_name, "..")) continue;
            snprintf(path, sizeof(path), "%s/%s", folder2, ent->d_name);
            if (stat(path, &st) == 0 && !S_ISDIR(st.st_mode)) {
                int l = (int)strlen(ent->d_name);
                if (l > max) max = l;
            }
        }
        closedir(d);
    }
    return max;
}

int file_exists_in_folder1(const char* folder1, const char* filename) {
    char path[1024];
    struct stat st;
    snprintf(path, sizeof(path), "%s/%s", folder1, filename);
    if (stat(path, &st) == 0 && !S_ISDIR(st.st_mode)) return 1;
    return 0;
}

void print_status_line(const char* color, const char* status, const char* filename,
                       const char* suffix, int left_pad,
                       int status_col_width, int filename_col_width) {
    if (left_pad > 0) printf("%*s", left_pad, "");
    printf("%s%-*s" NC " ", color, status_col_width, status);
    printf("%-*s", filename_col_width, filename);
    if (suffix && suffix[0]) printf("%s", suffix);
    printf("\n");
}

int main(int argc, char* argv[]) {
    if (argc == 2 && (!strcmp(argv[1], "-h") || !strcmp(argv[1], "--help"))) {
        print_help(argv[0]);
        return 0;
    }
    if (argc != 3) {
        fprintf(stderr, "Usage: %s <folder1> <folder2>\n", argv[0]);
        fprintf(stderr, "Try '%s --help' for more information.\n", argv[0]);
        return 1;
    }
    const char* folder1 = argv[1];
    const char* folder2 = argv[2];

    int term_width = get_term_width();
    int total = 0, match = 0, diff = 0, missing = 0, extra = 0;

    int max_fname = compute_max_filename_len(folder1, folder2);
    if (max_fname < 1) max_fname = 1;

    int status_col_width = 11;
    const int max_suffix_len = 17;
    int content_width = status_col_width + 1 + max_fname + max_suffix_len;
    int left_pad = (term_width - content_width) / 2;
    if (left_pad < 0) left_pad = 0;

    center("===============================================", term_width);
    center("Folder File Comparison Utility by sms1sis", term_width);
    center("===============================================", term_width);
    printf("\n");
    char buf[1024];
    snprintf(buf, sizeof(buf), "Comparing files in folders:");
    center(buf, term_width);
    snprintf(buf, sizeof(buf), "Folder 1: %s", folder1);
    center(buf, term_width);
    snprintf(buf, sizeof(buf), "Folder 2: %s", folder2);
    center(buf, term_width);
    center("-----------------------------------------------", term_width);
    printf("\n");

    DIR* d = opendir(folder1);
    if (!d) { perror(folder1); return 2; }
    struct dirent* ent;
    char path1[1024], path2[1024];
    struct stat st;
    while ((ent = readdir(d))) {
        if (!strcmp(ent->d_name, ".") || !strcmp(ent->d_name, "..")) continue;
        snprintf(path1, sizeof(path1), "%s/%s", folder1, ent->d_name);
        if (stat(path1, &st) != 0) continue;
        if (S_ISDIR(st.st_mode)) continue;

        total++;
        snprintf(path2, sizeof(path2), "%s/%s", folder2, ent->d_name);
        if (access(path2, F_OK) == 0) {
            char *h1 = file_sha256(path1);
            char *h2 = file_sha256(path2);
            if (h1 && h2 && strcmp(h1, h2) == 0) {
                print_status_line(GREEN, "[MATCH]", ent->d_name, "", left_pad, status_col_width, max_fname);
                match++;
            } else {
                print_status_line(RED, "[DIFF]", ent->d_name, "", left_pad, status_col_width, max_fname);
                diff++;
            }
            free(h1); free(h2);
        } else {
            print_status_line(YELLOW, "[MISSING]", ent->d_name, " not found in Folder2", left_pad, status_col_width, max_fname);
            missing++;
        }
    }
    closedir(d);

    d = opendir(folder2);
    if (d) {
        while ((ent = readdir(d))) {
            if (!strcmp(ent->d_name, ".") || !strcmp(ent->d_name, "..")) continue;
            snprintf(path2, sizeof(path2), "%s/%s", folder2, ent->d_name);
            if (stat(path2, &st) != 0) continue;
            if (S_ISDIR(st.st_mode)) continue;
            if (!file_exists_in_folder1(folder1, ent->d_name)) {
                print_status_line(CYAN, "[EXTRA]", ent->d_name, " only in Folder2", left_pad, status_col_width, max_fname);
                extra++;
            }
        }
        closedir(d);
    }

    printf("\n");
    center("-----------------------------------------------", term_width);
    center("Summary", term_width);
    center("-----------------------------------------------", term_width);

    // Perfectly aligned colon in summary
    const char *labels[] = {
        "Total files checked",
        "Matches",
        "Differences",
        "Missing in Folder2",
        "Extra in Folder2"
    };
    int values[] = {total, match, diff, missing, extra};
    int label_count = sizeof(labels) / sizeof(labels[0]);

    // Find the max label width for correct colon alignment
    int label_width = 0;
    for (int i = 0; i < label_count; ++i) {
        int l = (int)strlen(labels[i]);
        if (l > label_width) label_width = l;
    }

    // Print summary with aligned colons
    for (int i = 0; i < label_count; ++i) {
        // Ensures the colon is always aligned using label_width
        // The colon will be at (label_width + 1) column for all lines
        snprintf(buf, sizeof(buf), "%-*s : %d", label_width, labels[i], values[i]);
        center(buf, term_width);
    }

    center("===============================================", term_width);
    return 0;
}
