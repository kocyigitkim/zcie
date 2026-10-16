import os


def build_report(path):
    return Report(path).run()


class Report:
    def __init__(self, path):
        self.path = path

    def run(self):
        return os.path.basename(self.path)
