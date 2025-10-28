#!/usr/bin/env python3
import argparse
import subprocess


def png2pam(png_path: str) -> bytes:
    result = subprocess.run(
        [
            "pngtopam",
            "-alphapam",
            png_path,
        ],
        stdout=subprocess.PIPE,
        text=False,
        check=True,
    )
    return result.stdout


def perform_conversion(input_png_path: str, output_bin_path: str) -> None:
    END_HEADER_CHUNK = b"\nENDHDR\n"

    # first, convert to PAM
    pam = png2pam(input_png_path)
    if not pam.startswith(b"P7\n"):
        raise ValueError("pngtopam returned an invalid PAM file")
    end_header_start_index = pam.index(END_HEADER_CHUNK)
    end_header_index = end_header_start_index + len(END_HEADER_CHUNK)

    pam_data = pam[end_header_index:]
    if len(pam_data) != 4 * 32 * 32:
        raise ValueError("PAM payload has the wrong length, is this a 32x32 ARGB8888 icon?")

    # rearrange bytes
    argb_data_list = []
    pam_iter = iter(pam_data)
    while True:
        r = next(pam_iter, None)
        if r is None:
            break
        g = next(pam_iter)
        b = next(pam_iter)
        a = next(pam_iter)

        argb_data_list.append(a)
        argb_data_list.append(r)
        argb_data_list.append(g)
        argb_data_list.append(b)
    argb_data = bytes(argb_data_list)

    # write out
    with open(output_bin_path, "wb") as f:
        f.write(argb_data)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        dest="input_png",
    )
    parser.add_argument(
        dest="output_bin",
    )
    args = parser.parse_args()
    perform_conversion(args.input_png, args.output_bin)


if __name__ == "__main__":
    main()
