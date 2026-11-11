// The window process: one per user. Listens on $XDG_RUNTIME_DIR/fili-term/ui.sock so a second `fili-term` opens
// its window here instead of starting another Qt process, and creates the windows from TermWindow.qml.
#pragma once
#include <QLocalServer>
#include <QObject>
#include <QStringList>

class QQmlEngine;
class QQmlComponent;

class Instance : public QObject {
    Q_OBJECT
public:
    Instance(QQmlEngine* engine, QObject* parent = nullptr);
    // "open\t<attach id>\t<cwd>\t<argv…>"
    void handle(const QString& request);
    Q_INVOKABLE void newWindow(const QString& cwd);

private:
    void open(int attach, const QString& cwd, const QStringList& command);
    QQmlEngine* m_engine;
    QQmlComponent* m_window = nullptr;
    QLocalServer m_server;
};
